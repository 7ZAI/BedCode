/**
 * 插件前端错误消费工具（ADR 0030 错误信封，插件侧）
 *
 * 宿主桥（`wasm_core/manager/host/commands.rs::plugin_command_error`）把插件
 * 命令失败统一转换为信封 rejection（`{code, request_id, params}`，与宿主
 * `src/utils/userError.ts` 的 ErrorEnvelope 同构）。本工具在插件前端做两件事：
 * - `parsePluginError`：把任意 rejection 解析为信封（畸形 / 字符串 → null）
 * - `resolvePluginErrorText`：信封 code → 友好文案；未注册 / 非信封 → fallbackKey
 *
 * code 约定 = 插件 i18n **注册后的完整 key**（`<plugin_id>.<namespace>.<name>`，
 * 如 `com.bedcode.agent-hub.install.error.busy`）：插件 `registerMessages` 以
 * 扁平 key merge 进宿主 i18n，故直接 `hostI18n.global.t(code)` 可解析；未注册时
 * vue-i18n 返回 key 原文，用「t !== code」判定（与宿主 lookupMessage 同判据）。
 *
 * 硬不变量（与宿主 showUserError 同口径）：技术详情（request_id / 原文 / 堆栈）
 * **永不**进返回文案——调用方自行 `console.error` 落日志。
 */
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

/** 宿主 AppError 信封形状（ADR 0030；与宿主 src/utils/userError.ts 同构） */
export interface PluginErrorEnvelope {
  code: string
  request_id?: string
  params?: Record<string, unknown>
}

/** 解析 invoke rejection → 信封；畸形 / 字符串 / null → null（调用方落兜底） */
export function parsePluginError(e: unknown): PluginErrorEnvelope | null {
  if (e && typeof e === 'object') {
    const code = (e as { code?: unknown }).code
    if (typeof code === 'string') {
      const requestId = (e as { request_id?: unknown }).request_id
      const params = (e as { params?: unknown }).params
      return {
        code,
        request_id: typeof requestId === 'string' ? requestId : undefined,
        params:
          params && typeof params === 'object'
            ? (params as Record<string, unknown>)
            : undefined,
      }
    }
  }
  return null
}

/**
 * 信封 code → 插件友好文案；未命中 → fallbackKey → 宿主通用兜底。
 *
 * 解析顺序（零映射层，与宿主 lookupMessage 同判据「t 未注册返回 key 原文」）：
 * 1. `errors.<code>`——宿主机制码命名空间（host.* / frontend.*，宿主 locale 注册）
 * 2. `<code>` 裸完整 key——插件业务码（registerMessages 扁平 merge 进宿主 i18n）
 * 3. `<context.id>.<fallbackKey>`——fallback 短 key 补全插件前缀（约定 fallbackKey
 *    为插件自身短 key，如 agent-hub `hub.inst.failed` / ai-chatbox `testFailed`）
 * 4. `errors.frontend.internal`——兜底不可命中（漏注册 / 测试 mock）时返回宿主
 *    通用文案；**永不输出 key / code 原文**
 */
export function resolvePluginErrorText(
  context: PluginContext,
  e: unknown,
  fallbackKey: string,
): string {
  const hostI18n = context.i18n.getI18n()
  const env = parsePluginError(e)
  if (env) {
    const hostResolved = hostI18n?.global?.t?.(`errors.${env.code}`, env.params ?? {})
    if (typeof hostResolved === 'string' && hostResolved !== `errors.${env.code}`) {
      return hostResolved
    }
    const text = hostI18n?.global?.t?.(env.code, env.params ?? {})
    if (typeof text === 'string' && text !== env.code) return text
  }
  // fallback 补全插件前缀直查（不依赖 context.i18n.t：宿主不可用时它返回 key 原文，
  // 会把「未注册」误判为命中而输出完整 key）
  const fullKey = `${context.id}.${fallbackKey}`
  const fallback = hostI18n?.global?.t?.(fullKey)
  if (typeof fallback === 'string' && fallback !== fullKey) return fallback
  const internal = hostI18n?.global?.t?.('errors.frontend.internal')
  return typeof internal === 'string' && internal !== '' ? internal : ''
}
