/**
 * 插件运行时事件监听统一注册（main.ts 收敛产物）
 *
 * 宿主侧三个插件事件通道的监听集中于此，main.ts 只调用一次 setup：
 * - plugin:notify：host_notify Host Function 发送的插件通知（业务通知，维持原状）
 * - plugin:error：插件自检失败（如 hooks 脚本拷贝失败）→ 友好提示，插件状态不变
 * - plugin:runtime-error：插件 WASM 调用 panic / trap / 自动恢复失败 → 友好提示
 *
 * 两个错误通道的载荷都是**错误信封**（ADR 0030 决定 7，形状与 IPC 拒绝一致：
 * `{ code, request_id, params? }`），与 invoke rejection 共用消费层
 * （`parseInvokeError` / `showUserError`）：toast 只出 `errors.<code>` 模板文案
 * （params 插值应用显示名），技术详情**只存在于宿主日志**——前端不持有、不展示，
 * 追踪号 request_id 记进前端日志，可与宿主日志里的详情按值关联。
 *
 * 回调内 useToast() 在事件触发时求值（Pinia 已安装），与 main.ts 原逻辑一致。
 */
import { listen } from '@tauri-apps/api/event'
import { useToast } from '@/composables/useToast'
import { parseInvokeError, showUserError } from '@/utils/userError'

interface PluginNotifyPayload {
  plugin_id: string
  title: string
  body: string
}

/**
 * 事件通道错误信封载荷（宿主侧产生，字段名与 `system::error::EventEnvelope::payload`
 * 一致）。`params` 只含已消毒的显示名参数（`name` / `plugin`），无技术详情。
 *
 * 字段全为可选：载荷畸形 / 缺 code（遗留形状）时由 `parseInvokeError` 落
 * `host.internal` 兜底文案，不抛错、不静默吞。
 */
interface PluginErrorEnvelopePayload {
  code?: string
  request_id?: string
  params?: Record<string, unknown>
}

/** 注册插件事件监听（main.ts 启动时调用一次，幂等） */
export function setupPluginRuntimeListeners() {
  // 监听插件通知事件（由 host_notify Host Function 发送）
  // 业务通知（非错误事件）：按 ADR 0030 决定 7 维持现状；约定见插件开发检查清单
  // ——通知文案不得携带技术详情
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
  // 配置失败（如 hooks 脚本拷贝失败）→ 友好提示，插件状态不变
  listen<PluginErrorEnvelopePayload>('plugin:error', (event) => {
    showUserError(parseInvokeError(event.payload))
  })

  // 监听插件运行时异常事件（宿主 WASM 调用 panic / trap / 自动恢复失败时发送）
  // 文案按语义码走 errors.host.plugin.*：同一 kind 对用户是同一件事（同一段文案），
  // 详情（含 panic 消息 / 回溯）不再进 toast，也不截断展示——只在宿主日志里
  listen<PluginErrorEnvelopePayload>('plugin:runtime-error', (event) => {
    showUserError(parseInvokeError(event.payload))
  })
}
