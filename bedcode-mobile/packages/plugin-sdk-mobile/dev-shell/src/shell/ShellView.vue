<template>
  <div class="flex flex-col h-full min-h-0 mobile-app mobile-ui bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-primary)]">
    <ShellHost />
  </div>
</template>

<script setup lang="ts">
/**
 * Dev Shell 宿主壳入口（宿主 ShellView.vue 的同形实现）
 * -----------------------------------------------------------------------------
 * 这一层只做三件事：注册应用数据源、首次拉清单、卸载时回收。其余全部交给
 * ShellHost——入口越薄，将来换数据源时改动面越小。
 *
 * 与宿主的三处差异（都是浏览器环境使然，不改变壳的形态）：
 *   · 无安全区注入：宿主读 Tauri 的安全区值，浏览器的 env() 已由 mobile.css 兜住
 *   · 无 100dvh 容器：高度由 App.vue 的手机视口容器承担（宿主承担者是 ShellView 自己）
 *   · 数据源是 dev-shell 的调试记录（内置应用 + 被调试插件），不是宿主插件运行时
 */
import { onMounted, onUnmounted } from 'vue'
import ShellHost from './components/ShellHost.vue'
import { createDevAppSource, DEV_APP_SOURCE_ID } from './adapters/devAppSource'
import { activatePlugin, deactivatePlugin } from '../loader'
import { useShellApps } from './composables/useShellApps'
import { getShellRegistry } from './registry'
import { logger } from './logger'
import type { Disposable } from './types'

const registry = getShellRegistry()
const { refresh } = useShellApps()

let sourceDisposable: Disposable | null = null

onMounted(async () => {
  // 数据源在挂载时注册：壳本身不 import 任何具体形态的实现细节
  sourceDisposable = registry.registerSource(
    createDevAppSource({ activate: activatePlugin, deactivate: deactivatePlugin }),
  )
  try {
    await refresh()
  } catch (e) {
    // refresh 内部已逐源兜底并记录，这里只兜住意外抛出，避免挂了白屏
    logger.error(`initial refresh failed: ${String(e)}`)
  }
})

onUnmounted(() => {
  // 对称回收：先清该源写入的应用，再摘数据源，避免留下无主的记录
  registry.clearSource(DEV_APP_SOURCE_ID)
  sourceDisposable?.dispose()
  sourceDisposable = null
})
</script>