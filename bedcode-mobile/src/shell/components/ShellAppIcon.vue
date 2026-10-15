<template>
  <span
    class="inline-flex items-center justify-center flex-shrink-0 overflow-hidden border"
    :class="toneClass"
    :style="boxStyle"
    role="img"
    :aria-label="app.name"
  >
    <!-- 图片图标：清单里写的是相对插件目录的路径（icon.svg / icon.png …），
         经 asset protocol 取图；加载失败退回首字母，不留空白格 -->
    <img
      v-if="kind === 'image' && app.iconUrl && !imageFailed"
      :src="app.iconUrl"
      :alt="app.name"
      class="w-full h-full object-cover"
      @error="imageFailed = true"
    />
    <!-- SVG path d（应用自带图标，24×24 视框 stroke 风格） -->
    <svg
      v-else-if="kind === 'path'"
      :width="innerSize"
      :height="innerSize"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path :d="app.icon" />
    </svg>
    <!-- emoji 图标 -->
    <span
      v-else-if="kind === 'emoji'"
      :style="{ fontSize: `${innerSize}px`, lineHeight: '1' }"
      aria-hidden="true"
      >{{ app.icon }}</span
    >
    <!-- 无图标 / 图片加载失败：首字母回退（中文名取首字比取不到拉丁字母更可读） -->
    <span
      v-else
      :style="{ fontSize: `${Math.round(size * 0.42)}px`, fontWeight: 600 }"
      aria-hidden="true"
      >{{ initial }}</span
    >
  </span>
</template>

<script setup lang="ts">
/**
 * 应用图标
 *
 * 四种形态归一：图片路径 / emoji / SVG path d / 首字母回退。判据集中在 utils.iconKindOf，
 * 避免各屏各写一份「这是不是 path」的判断。
 */
import { computed, ref, watch } from 'vue'
import type { ShellApp } from '../types'
import { iconKindOf, initialOf } from '../utils'

const props = withDefaults(
  defineProps<{
    app: ShellApp
    /** 图标盒边长（px） */
    size?: number
    /** 色调：accent（强调底）/ plain（中性底）/ ghost（幽灵框，用于「更多」占位） */
    tone?: 'accent' | 'plain' | 'ghost'
  }>(),
  { size: 44, tone: 'accent' },
)

const kind = computed(() => iconKindOf(props.app.icon))
const initial = computed(() => initialOf(props.app.name))
/**
 * 图形占盒比例：三种形态各自留白不同，同一盒子才显得是一套。
 * SVG 线稿本身有内边距，占 0.55 即视觉接近满格；首字母是实心字形，占 0.42；
 * emoji 是插件清单里自带的品牌位，字面大、视觉重量最重，压到 0.52——
 * 否则彩色 emoji 会在同为 44px 的格子里比旁边克制的线稿图标明显更响。
 */
const innerSize = computed(() => {
  const ratio = kind.value === 'emoji' ? 0.52 : 0.55
  return Math.round(props.size * ratio)
})

/** 图片加载失败标记：插件换图标后重置，避免停在首字母回退 */
const imageFailed = ref(false)
watch(
  () => [props.app.icon, props.app.iconUrl],
  () => {
    imageFailed.value = false
  },
)

/** 圆角随尺寸线性放大并钳制：小盒（列表行）接近方形角，大可盒（预览位）过渡到拟 Windows/Android 大圆角，避免同一图标在 12/16 两档间跳变 */
const borderRadius = computed(() => {
  const r = Math.round(props.size * 0.25)
  return `${Math.min(18, Math.max(10, r))}px`
})

const boxStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  borderRadius: borderRadius.value,
}))

const toneClass = computed(() => {
  switch (props.tone) {
    case 'plain':
      return 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)] border-[var(--mobile-border)]'
    case 'ghost':
      return 'bg-transparent text-[var(--mobile-text-secondary)] border-dashed border-[var(--mobile-border-hover)]'
    default:
      // accent 底板对四种形态取同一个表面 + 同一条描边，只有图形本身不同。
      // 早前 emoji / 图片走中性底、首字母回退走 accent 淡底，同一宫格里出现
      // 深色块与浅灰块两种质感，读起来像两种控件；这里收成一套，格子只由
      // 里面的图形区分，不靠底色。emoji / 图片自带颜色，不叠前景色，
      // 否则会出现双色描边。
      return 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-primary)] border-[var(--mobile-border)]'
  }
})
</script>
