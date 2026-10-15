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
 * 这一层只做四件事：注册应用数据源、首次拉清单、卸载时摘数据源（清单记录不随卸载清，
 * 见 onUnmounted 注释）、自持布局框（安全区/高度链）。其余全部交给 ShellHost
 * ——入口越薄，将来替换数据源时改动面越小。
 *
 * 安全区与平台信息由 App.vue provide 注入（同一进程级真源，壳不另起一份）。
 */
import { computed, inject, onMounted, onUnmounted, ref, type Ref } from 'vue'
import { logger } from '@/utils/frontendLogger'
import ShellHost from '../components/ShellHost.vue'
import { createPluginAppSource } from '../adapters/pluginAppSource'
import { getShellRegistry } from '../registry'
import { useShellApps } from '../composables/useShellApps'
import type { Disposable } from '../types'
import '../styles/shell.css'
import '@/styles/wallpaper.css'

const registry = getShellRegistry()
const { refresh, apps } = useShellApps()

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

/**
 * 容器样式：仅移动形态下补顶部安全区；底部安全区由壳内底部元素各自 padding 承担。
 *
 * 同时把顶部内边距量以 `--shell-safe-top` 暴露出去：状态栏那条带子是容器的
 * padding 区，壁纸与氛围层都画在 `.shell-ui` 里（padding 之下），不补这一下
 * 状态栏下方会留一条与内容区不同色的硬边。壁纸据此向上越界内边距满幅铺开。
 */
const containerStyle = computed(() => {
  if (!platformInfo.value.isMobile) return {}
  const top = safeArea.value.top || 0
  return { paddingTop: `${top}px`, '--shell-safe-top': `${top}px` }
})

let sourceDisposable: Disposable | null = null
/** 冷启动清单轮询定时器（应用扫描未就绪时的补救） */
let listRetryTimer: ReturnType<typeof setTimeout> | null = null

/**
 * 冷启动清单轮询：后端插件扫描是异步的，壳首次 refresh 可能撞上「还没扫完」而拿到空清单，
 * 表现为首页「0 个应用 / 尚未安装 WASM 应用」且不再自愈（实测冷启动复现）。
 * 这里只补「首次为空」这一种情形，拿到非空清单立即停：不把轮询做成常驻心跳。
 * 参数与 PluginLoader 的启动扫描轮询同口径（250ms × 20s）。
 */
const LIST_RETRY_POLL_MS = 250
const LIST_RETRY_TIMEOUT_MS = 20_000

function scheduleListRetry(startedAt: number): void {
  if (Date.now() - startedAt > LIST_RETRY_TIMEOUT_MS) {
    logger.warn('[ShellView] app list still empty after startup timeout')
    return
  }
  listRetryTimer = setTimeout(async () => {
    try {
      await refresh()
    } catch (e) {
      logger.error('[ShellView] app list retry failed', e)
    }
    if (apps.value.length === 0) scheduleListRetry(startedAt)
  }, LIST_RETRY_POLL_MS)
}

onMounted(async () => {
  // 数据源在挂载时注册：壳本身不 import 任何具体形态的实现细节，
  // 只有这个入口知道「当前真源是插件系统」
  sourceDisposable = registry.registerSource(createPluginAppSource())
  const startedAt = Date.now()
  try {
    await refresh()
  } catch (e) {
    // refresh 内部已逐源兜底并记录，这里只兜住意外抛出，避免挂了白屏
    logger.error('[ShellView] initial refresh failed', e)
  }
  if (apps.value.length === 0) scheduleListRetry(startedAt)
})

onUnmounted(() => {
  if (listRetryTimer !== null) {
    clearTimeout(listRetryTimer)
    listRetryTimer = null
  }
  // 只摘数据源，**不清清单记录**：壳内屏幕栈是模块级单例、跨挂载存活，从插件整页
  // 路由（如应用设置页）返回时重挂的 ShellView 立即渲染栈顶的 app-run 屏——清掉记录
  // 会让它在这段窗口把「清单还没重拉回来」判成「应用不存在」（2026-10-16 用户反馈）。
  // 清单新鲜度由挂载时的 refresh 对账（upsertApps 全量覆盖，未返回的应用自然回收）。
  sourceDisposable?.dispose()
  sourceDisposable = null
})
</script>
