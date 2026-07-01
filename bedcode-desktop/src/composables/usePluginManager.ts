/**
 * Plugin Manager Composable
 *
 * 插件管理页面业务逻辑 — 加载列表、切换启用、展开详情、复制路径
 */

import { ref } from 'vue'
import { pluginListLoaded, pluginActivate, pluginDeactivate } from '@/plugin/commands'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import type { PluginInfo, PluginState } from '@/plugin/types'

/** 获取插件状态的显示文本 key */
function getStateKey(state: PluginState): string {
  if (state.state === 'Error') return 'desktop.plugin.error'
  if (state.state === 'Activated') return 'desktop.plugin.activated'
  if (state.state === 'Loaded') return 'desktop.plugin.loaded'
  if (state.state === 'Deactivated') return 'desktop.plugin.deactivated'
  return 'desktop.plugin.loaded'
}

/** 判断插件是否为激活状态 */
function isActivated(state: PluginState): boolean {
  return state.state === 'Activated'
}

/** 判断插件是否为错误状态 */
function isErrorState(state: PluginState): boolean {
  return state.state === 'Error'
}

/** 获取错误信息 */
function getErrorMessage(state: PluginState): string {
  if (state.state === 'Error') return state.error || ''
  return ''
}

/** 生成 contributes 摘要文本 */
function getContributesSummary(plugin: PluginInfo): string {
  const parts: string[] = []
  const c = plugin.contributes
  if (!c) return '—'
  if (c.commands?.length) parts.push(`${c.commands.length} commands`)
  if (c.views?.length) parts.push(`${c.views.length} views`)
  if (c.terminal) parts.push('terminal')
  if (c.toolProviders?.length) parts.push(`${c.toolProviders.length} tools`)
  if (c.fileHandlers?.length) parts.push(`${c.fileHandlers.length} handlers`)
  return parts.length > 0 ? parts.join(' · ') : '—'
}

export function usePluginManager() {
  const toast = useToast()
  const t = i18n.global.t

  const plugins = ref<PluginInfo[]>([])
  const loading = ref(false)
  const expandedId = ref<string | null>(null)

  /** 加载插件列表 */
  async function loadPlugins(): Promise<void> {
    loading.value = true
    try {
      plugins.value = await pluginListLoaded()
    } catch (e: any) {
      toast.error(t('desktop.plugin.loadFailed'))
    } finally {
      loading.value = false
    }
  }

  /** 切换插件启用/停用 */
  async function togglePlugin(id: string, enable: boolean): Promise<boolean> {
    try {
      if (enable) {
        await pluginActivate(id)
      } else {
        await pluginDeactivate(id)
      }
      // 重新加载列表以获取最新状态
      await loadPlugins()
      return true
    } catch (e: any) {
      const key = enable ? 'desktop.plugin.activateFailed' : 'desktop.plugin.deactivateFailed'
      toast.error(t(key, { error: e.message || 'Unknown error' }))
      return false
    }
  }

  /** 切换展开/折叠 */
  function toggleExpand(id: string): void {
    expandedId.value = expandedId.value === id ? null : id
  }

  /** 复制扩展路径到剪贴板 */
  async function copyPath(path: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(path)
      toast.success(t('desktop.plugin.pathCopied'))
    } catch {
      toast.error(t('desktop.plugin.copyFailed'))
    }
  }

  return {
    plugins,
    loading,
    expandedId,
    loadPlugins,
    togglePlugin,
    toggleExpand,
    copyPath,
    // 工具函数导出供模板使用
    getStateKey,
    isActivated,
    isErrorState,
    getErrorMessage,
    getContributesSummary,
  }
}
