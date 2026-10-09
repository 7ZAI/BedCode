/**
 * 宿主页域错误处理机制（票 2026-10-09：旧宿主机制在新前端的对齐）
 * -----------------------------------------------------------------------------
 * 复制而非 import 宿主实现（插件前端不得 import 宿主 `@/` 路径，且插件包需自足）。
 * 三条口径与旧宿主逐字同源：
 *   1. **连接错误分类** `classifyConnectionError` —— 契约与 `src/utils/connectionError.ts`
 *      完全一致（分类顺序、中文「超时」兼容、大小写敏感）。
 *   2. **分类 → 域内 i18n key**（`hub.*`）：toast 文案统一从 `CONNECTION_ERROR_KEYS` 取，
 *      禁止各处自行 if/else 拼文案（旧 DevicesView 的 startConnection / connectFromScanResult
 *      两处同款分类就是被抽成纯函数防漂移的）。
 *   3. **插件命令结果归一** `ensureCommandOk`：`{code,message,data}` 形状，`code` 为 0
 *      或未给出视为成功；`code!==0` → 抛 `message || fallbackKey`。调用方一律
 *      `t(err.message)` 渲染 ——「错误槽位存 i18n key 或原始文案」是旧前端既定口径
 *      （见旧 DevicesView `connectionError` 注释）。
 */

/** 错误分类结果：timeout / refused / unreachable / other */
export type ConnectionErrorKind = 'timeout' | 'refused' | 'unreachable' | 'other'

/**
 * 按错误消息分类连接错误（兼容中文「超时」；大小写敏感，与旧宿主行为一致）。
 * 分类顺序不可调换：timeout → refused → unreachable → other。
 */
export function classifyConnectionError(errorMsg: unknown): ConnectionErrorKind {
  const msg = String(errorMsg ?? '')
  if (msg.includes('timeout') || msg.includes('超时')) return 'timeout'
  if (msg.includes('refused') || msg.includes('rejected')) return 'refused'
  if (msg.includes('unreachable') || msg.includes('network')) return 'unreachable'
  return 'other'
}

/** 分类 → 域内 toast 文案 key（`hub.*`；双语由 host/i18n.ts 维护） */
export const CONNECTION_ERROR_KEYS: Record<ConnectionErrorKind, string> = {
  timeout: 'hub.timeoutToast',
  refused: 'hub.refusedToast',
  unreachable: 'hub.unreachableToast',
  other: 'hub.connectFailedToast',
}

/** 连接错误 → 展示文案 key（页面 toast / 错误槽位一律用它） */
export function connectionErrorKey(errorMsg: unknown): string {
  return CONNECTION_ERROR_KEYS[classifyConnectionError(errorMsg)]
}

/** 插件 HTTP 类命令结果最小形状（与宿主会话命令面 `{code,message,data}` 一致） */
export interface CommandResult<T = unknown> {
  code?: number
  message?: string
  data?: T
}

/**
 * 命令结果归一：`code` 为 0 或未给出视为成功；`code!==0` → 抛 `message || fallbackKey`。
 *
 * 返回值形状不变（auth 类命令返回 `{accepted}`、会话类返回 `{code,message,data}`，
 * 两者都直接透传），只做「失败即抛」的判定。
 *
 * @param result 命令返回值（undefined/null 按成功处理 —— 前端 handler 可无返回）
 * @param fallbackKey 该动作的兜底文案 key（如 'hub.startFailed'），服务端 message 优先
 */
export function ensureCommandOk<R extends object>(
  result: R | null | undefined,
  fallbackKey: string,
): R | undefined {
  const res = result as CommandResult | null | undefined
  if (res && res.code !== undefined && res.code !== 0) {
    throw new Error(res.message || fallbackKey)
  }
  return result ?? undefined
}
