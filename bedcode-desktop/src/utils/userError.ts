/**
 * 前端错误消费层（ADR 0030 错误信封 & 用户提示边界）
 *
 * 收敛点：全部 Tauri invoke rejection / 事件失败 / renderer 未知异常统一归一到
 * `UserError { code, request_id, params? }`，三个收敛函数：
 * - `parseInvokeError(e)`   invoke rejection 解析（对象信封 | 遗留字符串，expand 阶段双兼容）
 * - `showUserError(err, opts)`  集中友好 toast + 日志落盘；永不渲染 code / request_id / 技术详情
 * - `userErrorFromUnknown(e)`   renderer 侧未知异常 → frontend.internal
 *
 * UI 呈现规则（硬）：toast / 页面只显示 friendly 文案（`errors.<code>` 模板 + params 插值），
 * 不显示任何错误码；技术详情唯一的落点是产生方进程日志（Rust tracing / 前端 logger）。
 */

import { toast } from 'vue-sonner'
import i18n from '@/locales'
import { logger } from '@/utils/frontendLogger'

/** 兜底码：未映射 / 畸形形状一律落此（i18n key = `errors.host.internal`） */
export const HOST_INTERNAL_CODE = 'host.internal'
/** renderer 侧未知异常兜底码（i18n key = `errors.frontend.internal`） */
export const FRONTEND_INTERNAL_CODE = 'frontend.internal'
/** IPC 超时码——v1 唯一提供「重试」按钮的码（i18n key = `errors.host.invoke.timeout`） */
export const IPC_TIMEOUT_CODE = 'host.invoke.timeout'

/** 插件业务错误标记名（票 02 消费，宿主桥只做机制检测）；宿主 v0 基码不产出 */
export const ENVELOPE_MARKER = '__bedcode_error__'

/** 边界信封形状（与 Rust `AppError` Serialize 输出一致，见 src-tauri/src/system/error.rs） */
export interface ErrorEnvelope {
  code: string
  request_id?: string
  params?: Record<string, unknown>
}

/**
 * 前端统一错误类型：跨边界失败在 renderer 侧的规范形状。
 * `message` 仅为内部占位（存 code），**永不作展示用途**——展示由 showUserError 走 i18n。
 */
export class UserError extends Error {
  readonly code: string
  readonly requestId?: string
  readonly params?: Record<string, unknown>

  constructor(code: string, params?: Record<string, unknown>, requestId?: string) {
    super(code)
    this.name = 'UserError'
    this.code = code
    this.params = params
    this.requestId = requestId
  }
}

/** 形状守卫：信封对象（code 为字符串即可识别；畸形/缺失一律走兜底） */
function isEnvelopeShape(e: unknown): e is ErrorEnvelope {
  return typeof e === 'object' && e !== null && typeof (e as ErrorEnvelope).code === 'string'
}

/**
 * 解析 invoke rejection → UserError。
 *
 * expand 阶段兼容两种形状（ADR 0030 破坏面声明）：
 * - 对象信封 `{code, request_id?, params?}`（Rust AppError 序列化产物）→ 原样映射
 * - 其余任意形状（遗留字符串 / Error / null / 无 code 对象）→ `host.internal` 兜底
 */
export function parseInvokeError(e: unknown): UserError {
  if (e instanceof UserError) return e
  if (isEnvelopeShape(e)) {
    return new UserError(e.code, e.params, e.request_id)
  }
  // 兜底：本函数保持纯解析（不落日志——展示路径 showUserError 统一记录），
  // 便于形状矩阵测试与 dup 排查
  return new UserError(HOST_INTERNAL_CODE)
}

/** showUserError 选项 */
export interface ShowUserErrorOptions {
  /** `errors.<code>` 缺文案时的回退码（如 expand 阶段尚未落文案的业务码） */
  fallbackCode?: string
  /** 重试回调——仅 `host.invoke.timeout` 且提供回调时 toast 显示「重试」按钮 */
  retry?: () => void
}

/**
 * 集中友好提示：toast 显示 `errors.<code>` 文案（params 插值），logger 记录 code + request_id
 * + 原始错误（error 级，release 下同样转发落盘，见 frontendLogger）。
 *
 * 硬不变量：toast 文本永不包含 code / request_id / 技术详情；未知 code 回退 fallbackCode /
 * host.internal 文案。v1 仅 `host.invoke.timeout` 码支持重试按钮（须显式传 retry 回调）。
 *
 * @returns 归一化后的 UserError（调用方可继续处理 / 断言）
 */
export function showUserError(err: unknown, options?: ShowUserErrorOptions): UserError {
  const ue: UserError = err instanceof UserError ? err : parseInvokeError(err)
  const message = lookupMessage(ue.code, ue.params, options?.fallbackCode)

  // 日志：code + request_id + 原始错误全量（技术详情的唯一前端落点；
  // Error 自带 stack，envelope 对象原样序列化）
  logger.error(`[user-error] code=${ue.code} request_id=${ue.requestId ?? '-'}`, err)

  const toastOptions: Record<string, unknown> = { duration: 5000 }
  if (ue.code === IPC_TIMEOUT_CODE && options?.retry) {
    toastOptions.action = { label: t('errors.retry'), onClick: options.retry }
  }
  toast.error(message, toastOptions)
  return ue
}

/**
 * renderer 侧未知异常兜底（非 invoke 路径：纯前端逻辑抛错等）：
 * 日志留全量原文，返回 `frontend.internal` UserError。
 */
export function userErrorFromUnknown(e: unknown): UserError {
  logger.error('[user-error] unhandled renderer error:', e)
  return new UserError(FRONTEND_INTERNAL_CODE)
}

/** 懒取 t：i18n 实例可能在测试中被 mock（贫 mock 只有 locale 没有 t），
 * 模块加载期绑定会炸；改为调用点取用，行为不变。 */
function t(key: string, params?: Record<string, unknown>): string {
  return i18n.global.t(key, params ?? {})
}

/**
 * 翻译友好文案（ADR 0030 决定 4「code 即 i18n key」，零映射层）：
 * 1. `errors.<code>`（宿主域 / 前端域注册表）
 * 2. 裸 `<code>`——插件域注册码（`<plugin_id>.<namespace>.<name>`，插件 registerMessages
 *    后 key 即全文；宿主不预埋插件文案，只在这里回退查找）
 * 3. fallbackCode / host.internal 兜底（expand 阶段业务码尚未落文案）
 *
 * 注意：i18n 实例为运行时注入（测试/宿主可替换），**不做模块级 t 绑定**——
 * 调用点才取 `i18n.global.t`（沿用 lookupMessage 的懒访问，贫 mock 不炸模块加载）。
 */
function lookupMessage(
  code: string,
  params: Record<string, unknown> | undefined,
  fallbackCode?: string,
): string {
  const interpolate = (key: string): string => i18n.global.t(key, params ?? {})
  if (i18n.global.te(`errors.${code}`)) return interpolate(`errors.${code}`)
  // 插件注册码：key = code 全文（如 com.bedcode.terminal-session.session.error.sessionNotFound），
  // 插件 registerMessages 以扁平 key merge 进宿主 i18n。vue-i18n 的 `te` 只按点路径解析
  // 嵌套、不识别扁平索引，而 `t` 对未注册 key 返回 key 原文——用「t !== code」判定
  // 是否已注册（实证：te=false 但 t 能取到扁平 key 文案）。
  const pluginText = interpolate(code)
  if (pluginText !== code) return pluginText
  const fb = fallbackCode ?? HOST_INTERNAL_CODE
  return i18n.global.te(`errors.${fb}`)
    ? interpolate(`errors.${fb}`)
    : interpolate(`errors.${HOST_INTERNAL_CODE}`)
}