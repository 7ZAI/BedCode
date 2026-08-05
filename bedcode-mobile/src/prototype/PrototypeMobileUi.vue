<!--
  PROTOTYPE — 移动端 UI 重设计原型（一次性，供选型后删除）

  问题：移动端各页面风格/设计语言不统一，以设置页与插件页为参照重新设计整套 UI。
  方案：三个结构性差异显著的全应用变体，通过 ?variant=A|B|C 切换，
       每个变体内用静态 mock 数据模拟全部页面（连接/会话/工具箱/设置/插件/插件 nav tab）。

  - A 「秩序」：设置页分组行语言的全面推广（iOS 式分组 + 彩色图标 chip）
  - B 「空御」：状态优先的 Bento 控制中心（大磁贴 + 不对称网格）
  - C 「素黑」：终端驾驶舱（高密度、等宽数字、发丝线、零卡片）

  运行：cd bedcode-mobile && npm run dev → http://localhost:1420/prototype/mobile-ui
-->
<template>
  <div class="h-full bg-zinc-950 mobile-ui">
    <component :is="active.component" />
    <PrototypeSwitcher
      :model-value="variantKey"
      :label="active.label"
      @prev="cycle(-1)"
      @next="cycle(1)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import PrototypeSwitcher from './PrototypeSwitcher.vue'
import VariantA from './variants/VariantA.vue'
import VariantB from './variants/VariantB.vue'
import VariantC from './variants/VariantC.vue'

defineOptions({ name: 'PrototypeMobileUi' })

const VARIANTS = [
  { key: 'A', label: 'A · 秩序 分组列表', component: VariantA },
  { key: 'B', label: 'B · 空御 Bento 控制中心', component: VariantB },
  { key: 'C', label: 'C · 素黑 终端驾驶舱', component: VariantC },
]

const route = useRoute()
const router = useRouter()

const variantKey = computed(() => {
  const q = String(route.query.variant ?? 'A').toUpperCase()
  return VARIANTS.some((v) => v.key === q) ? q : 'A'
})

const active = computed(() => VARIANTS.find((v) => v.key === variantKey.value)!)

function setVariant(key: string) {
  router.replace({ query: { ...route.query, variant: key } })
}

function cycle(delta: number) {
  const idx = VARIANTS.findIndex((v) => v.key === variantKey.value)
  const next = (idx + delta + VARIANTS.length) % VARIANTS.length
  setVariant(VARIANTS[next].key)
}

/** 键盘左右键切换；聚焦输入控件时不拦截 */
function onKeydown(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null
  if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
  if (e.key === 'ArrowLeft') cycle(-1)
  if (e.key === 'ArrowRight') cycle(1)
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>
