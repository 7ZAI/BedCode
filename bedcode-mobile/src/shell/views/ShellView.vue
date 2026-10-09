<template>
  <!--
    壳自持布局框（票 2026-10-09 阶段 B）：旧宿主 MobileLayout 退役后，原先由它承担的
    三件事收归壳内——① 100dvh 全高容器（ShellHost 用 h-full 承接）② 顶部安全区
    padding（Android WebView 不支持 env(safe-area-inset-*)，完全依赖 JS 值）
    ③ `mobile-app mobile-ui` 祖先类（部分既有 CSS 选择器按这两个类命中）。
    安全区未就绪前只渲染占位底色，避免内容在状态栏下闪现（与旧 MobileLayout 同口径）。
  -->
  <div
    v-if="safeAreaReady"
    class="flex flex-col h-[100dvh] overflow-hidden mobile-app mobile-ui bg-[var(--mobile-bg-primary)]"
    :style="containerStyle"
  >
    <ShellHost />
  </div>
  <div v-else class="h-[100dvh] mobile-app mobile-ui bg-[var(--mobile-bg-primary)]" />
</template>

<script setup lang="ts">
/**
 * 宿主壳路由入口（/mobile/shell）
 *
 * 这一层只做四件事：注册应用数据源、首次拉清单、卸载时回收、自持布局框（安全区/高度链）。
 * 其余全部交给 ShellHost——入口越薄，将来替换数据源时改动面越小。
 *
 * 安全区与平台信息由 App.vue provide 注入（同一进程级真源，壳不另起一份）。
 */
import { computed, inject, onMounted, onUnmounted, ref, type Ref } from 'vue'
import { logger } from '@/utils/frontendLogger'
import ShellHost from '../components/ShellHost.vue'
import { createPluginAppSource, PLUGIN_APP_SOURCE_ID } from '../adapters/pluginAppSource'
import { getShellRegistry } from '../registry'
import { useShellApps } from '../composables/useShellApps'
import type { Disposable } from '../types'
import '../styles/shell.css'

const registry = getShellRegistry()
const { refresh } = useShellApps()

/**
 * App.vue 注入的宿主环境信息（缺省值用于单测 / 桌面预览：无安全区、非移动形态）。
 * 不引 `@/composables/*`：壳只消费 provide 的结果，不复制宿主机制。
 */
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar?: number }>>(
  'safeArea',
  ref({ top: 0, bottom: 0, navigationBar: 0 }),
)
const safeAreaReady = inject<Ref<boolean>>('safeAreaReady', ref(true))
const platformInfo = inject<Ref<{ isMobile: boolean }>>('platformInfo', ref({ isMobile: false }))

/** 容器样式：仅移动形态下补顶部安全区；底部安全区由壳内底部元素各自 padding 承担 */
const containerStyle = computed(() => {
  if (!platformInfo.value.isMobile) return {}
  return { paddingTop: `${safeArea.value.top || 0}px` }
})

let sourceDisposable: Disposable | null = null

onMounted(async () => {
  // 数据源在挂载时注册：壳本身不 import 任何具体形态的实现细节，
  // 只有这个入口知道「当前真源是插件系统」
  sourceDisposable = registry.registerSource(createPluginAppSource())
  try {
    await refresh()
  } catch (e) {
    // refresh 内部已逐源兜底并记录，这里只兜住意外抛出，避免挂了白屏
    logger.error('[ShellView] initial refresh failed', e)
  }
})

onUnmounted(() => {
  // 对称回收：先清该源写入的应用，再摘数据源，避免留下无主的记录
  registry.clearSource(PLUGIN_APP_SOURCE_ID)
  sourceDisposable?.dispose()
  sourceDisposable = null
})
</script>
