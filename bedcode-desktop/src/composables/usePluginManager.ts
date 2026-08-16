/**
 * Plugin Manager Composable
 *
 * 插件管理页面业务逻辑 — 加载列表、切换启停、复制路径
 * 开发模式下监听 plugin:dev-reload 事件触发热重载
 *
 * 启停遮罩态由 togglingId + togglingPluginInfo 驱动，View 层渲染全屏遮罩弹窗
 */

import { ref, computed, onMounted, onUnmounted } from 'vue'
import { pluginListLoaded } from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import { useToast } from '@/composables/useToast'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import i18n from '@/locales'
import type { PluginInfo } from '@/plugin/types'

/** 启停操作总超时：后端激活/停用含 hooks 清理（wsl.exe 桥接最长约 15s）与 fs 授权弹窗（30s），给足余量 */
const TOGGLE_TIMEOUT_MS = 30000

/** 遮罩最小展示时长：操作完成过快（WASM 插件毫秒级）时仍保持遮罩可见，避免弹窗瞬闪假象 */
const MIN_TOGGLE_VISIBLE_MS = 500

export function usePluginManager() {
  const toast = useToast()
  const t = i18n.global.t

  const plugins = ref<PluginInfo[]>([])
  const loading = ref(false)
  /** 正在切换启停的插件 id（用于遮罩与防重复点击） */
  const togglingId = ref<string | null>(null)
  /** 当前切换方向（true=启用，false=停用），配合 togglingId 显示遮罩文案 */
  const togglingDirection = ref<boolean>(true)

  /** 当前正在切换的插件信息（供遮罩弹窗显示名称） */
  const togglingPluginInfo = computed(() => {
    if (!togglingId.value) return null
    const p = plugins.value.find(p => p.id === togglingId.value)
    if (!p) return null
    const key = togglingDirection.value
      ? 'desktop.plugin.togglingEnable'
      : 'desktop.plugin.togglingDisable'
    return { id: p.id, name: p.name, message: t(key, { name: p.name }) }
  })

  // 开发模式热重载事件监听
  let devReloadUnlisten: UnlistenFn | null = null

  /** 加载插件列表 */
  async function loadPlugins(): Promise<void> {
    loading.value = true
    console.log('[PluginManager] loadPlugins() started')
    try {
      const result = await pluginListLoaded()
      console.log('[PluginManager] loadPlugins() received', result.length, 'plugin(s)')
      for (const p of result) {
        console.log(`[PluginManager]   - ${p.id} (state=${p.state.state}, type=${p.pluginType})`)
      }
      plugins.value = result
    } catch (e: any) {
      console.error('[PluginManager] loadPlugins() failed:', e)
      toast.error(t('desktop.plugin.loadFailed'))
    } finally {
      loading.value = false
    }
  }

  /** 切换插件启用/停用 */
  async function togglePlugin(id: string, enable: boolean): Promise<boolean> {
    if (togglingId.value) return false
    togglingId.value = id
    togglingDirection.value = enable
    const startedAt = Date.now()
    console.log(`[PluginManager] togglePlugin(${id}, enable=${enable})`)
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      const op = enable ? pluginLoader.activate(id) : pluginLoader.deactivate(id)
      const timeout = new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(t('desktop.plugin.toggleTimeout'))),
          TOGGLE_TIMEOUT_MS,
        )
      })
      // Tauri invoke 无超时机制，前端兜底：后端挂起（如 wsl.exe 桥接异常）时
      // 超时即报错并收起 loading，避免 spinner 无限转圈
      await Promise.race([op, timeout])
      // 重新加载列表以获取最新状态
      await loadPlugins()
      const name = plugins.value.find(p => p.id === id)?.name || id
      const key = enable ? 'desktop.plugin.enabledSuccess' : 'desktop.plugin.disabledSuccess'
      toast.success(t(key, { name }))
      console.log(`[PluginManager] togglePlugin(${id}) succeeded`)
      return true
    } catch (e: any) {
      const key = enable ? 'desktop.plugin.activateFailed' : 'desktop.plugin.deactivateFailed'
      console.error(`[PluginManager] togglePlugin(${id}) failed:`, e)
      toast.error(t(key, { error: e.message || 'Unknown error' }))
      return false
    } finally {
      clearTimeout(timer)
      // 遮罩最小展示时长：加载/卸载过快时延迟收起，避免全屏遮罩一闪而过造成闪烁假象
      const elapsed = Date.now() - startedAt
      if (elapsed < MIN_TOGGLE_VISIBLE_MS) {
        await new Promise((resolve) => setTimeout(resolve, MIN_TOGGLE_VISIBLE_MS - elapsed))
      }
      togglingId.value = null
    }
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

  // 开发模式：监听 plugin:dev-reload 事件，自动热重载前端 TS 模块
  onMounted(async () => {
    devReloadUnlisten = await listen<{ pluginId: string }>('plugin:dev-reload', async (event) => {
      const { pluginId } = event.payload
      console.log(`[PluginManager] Dev reload event: ${pluginId}`)
      await pluginLoader.reloadPlugin(pluginId)
      await loadPlugins()
    })
  })

  onUnmounted(() => {
    devReloadUnlisten?.()
    devReloadUnlisten = null
  })

  return {
    plugins,
    loading,
    togglingId,
    togglingPluginInfo,
    loadPlugins,
    togglePlugin,
    copyPath,
  }
}
