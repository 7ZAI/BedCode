<template>
  <ShellHost />
</template>

<script setup lang="ts">
/**
 * 宿主壳路由入口（/mobile/shell）
 *
 * 这一层只做三件事：注册应用数据源、首次拉清单、卸载时回收。
 * 其余全部交给 ShellHost——入口越薄，将来替换数据源时改动面越小。
 *
 * 与既有实现并存：本路由是新增的，既有的 /mobile/** 路由与页面不动，
 * 待 wasm-app 平台就绪后再决定切换默认入口。
 */
import { onMounted, onUnmounted } from 'vue'
import { logger } from '@/utils/frontendLogger'
import ShellHost from '../components/ShellHost.vue'
import { createPluginAppSource, PLUGIN_APP_SOURCE_ID } from '../adapters/pluginAppSource'
import { getShellRegistry } from '../registry'
import { useShellApps } from '../composables/useShellApps'
import type { Disposable } from '../types'
import '../styles/shell.css'

const registry = getShellRegistry()
const { refresh } = useShellApps()

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
