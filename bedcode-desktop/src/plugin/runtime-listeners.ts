/**
 * 插件运行时事件监听统一注册（main.ts 收敛产物）
 *
 * 宿主侧三个插件事件通道的监听集中于此，main.ts 只调用一次 setup：
 * - plugin:notify：host_notify Host Function 发送的插件通知
 * - plugin:error：插件自检失败（如 hooks 脚本拷贝失败）→ 弹窗提示
 * - plugin:runtime-error：插件 WASM 调用 panic / trap / 自动恢复失败
 *
 * 回调内 useToast() 在事件触发时求值（Pinia 已安装），与 main.ts 原逻辑一致。
 */
import { listen } from '@tauri-apps/api/event'
import { logger } from '@/utils/frontendLogger'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'

interface PluginNotifyPayload {
  plugin_id: string
  title: string
  body: string
}

interface PluginErrorPayload {
  plugin_id: string
  error: string
}

/** 插件运行时异常统一通道（宿主检测到插件异常时主动上报，见 PLUGIN_RUNTIME_ERROR） */
interface PluginRuntimeErrorPayload {
  plugin_id: string
  plugin_name: string
  kind: 'panic' | 'trap' | 'recovery_failed'
  error: string
}

/** 注册插件事件监听（main.ts 启动时调用一次，幂等） */
export function setupPluginRuntimeListeners() {
  // 监听插件通知事件（由 host_notify Host Function 发送）
  listen<PluginNotifyPayload>('plugin:notify', (event) => {
    const { title, body } = event.payload
    const toast = useToast()
    if (body) {
      toast.info(`${title}: ${body}`)
    } else {
      toast.info(title)
    }
  })

  // 监听插件自检失败事件（由 host_mark_plugin_error Host Function 发送）
  // 配置失败（如 hooks 脚本拷贝失败）→ 弹窗提示，插件状态不变
  listen<PluginErrorPayload>('plugin:error', (event) => {
    const { plugin_id, error } = event.payload
    const toast = useToast()
    logger.error(`[Plugin] ${plugin_id} self-check failed:`, error)
    toast.error(i18n.global.t('desktop.plugin.selfCheckFailed', { plugin: plugin_id, error }))
  })

  // 监听插件运行时异常事件（宿主 WASM 调用 panic / trap / 自动恢复失败时发送）
  // 按 kind 提示不同文案；error 细节可能很长（panic 消息/回溯），toast 截断展示，全量进 console
  listen<PluginRuntimeErrorPayload>('plugin:runtime-error', (event) => {
    const { plugin_id, plugin_name, kind, error } = event.payload
    const toast = useToast()
    logger.error(`[Plugin] ${plugin_name} (${plugin_id}) runtime error [${kind}]:`, error)
    const shortError = error.length > 120 ? `${error.slice(0, 120)}…` : error
    const key =
      kind === 'panic'
        ? 'desktop.plugin.runtimePanic'
        : kind === 'trap'
          ? 'desktop.plugin.runtimeTrap'
          : 'desktop.plugin.runtimeRecoveryFailed'
    toast.error(i18n.global.t(key, { name: plugin_name, error: shortError }))
  })
}
