/**
 * Android 系统返回键分发（宿主前端唯一的原生监听）
 * -----------------------------------------------------------------------------
 * 为什么需要这一层：
 *   Tauri AppPlugin 的行为是「只要 JS 侧存在 back-button listener，系统返回一律
 *   转发到 JS，不再执行默认的 webview 后退 / 退出 Activity」。因此宿主前端只能有
 *   **一个** 原生 listener，多个消费者（各应用 + 宿主壳）必须在 JS 侧排好序，
 *   否则同一次返回会让多个界面同时动作（应用退一级 + 平台退一屏）。
 *
 * 两层消费者与优先级（先应用、后平台）：
 *   ① 应用层：`context.ui.onBackPressed`（SDK 契约，file-transfer 的目录逐级退等）
 *   ② 平台层：宿主壳的兜底（覆盖层 → 屏栈回退 → 回主屏），见
 *      `src/shell/components/ShellHost.vue`
 *
 * 「消费」契约：处理器返回 `true` = 本次返回已被处理，后续处理器不再收到；
 * 返回 `undefined` / `false` = 本次不处理，交给下一层（应用不处理的返回会落到
 * 平台兜底，而不是被吞掉——这正是修复「应用内返回后无反应」的关键）。
 *
 * 监听生命周期：只在**存在订阅者**时持有原生 listener；最后一个订阅者取消时摘除，
 * 恢复系统默认返回行为（在平台主屏按返回 = 退出应用）。因此宿主壳的兜底订阅是
 * **按需**的：只有「有东西可回退」时才订阅（见 ShellHost 的 watchEffect），
 * 否则常驻订阅会让「主屏按返回退出应用」永远失效。
 *
 * 非 Tauri 环境（dev-shell 浏览器 / 单元测试）静默降级：动态 import 失败即视为
 * 无原生监听，`dispatch` 仍可被测试直接调用。
 */

import { logger } from '@/utils/frontendLogger'

/** 原生返回事件载荷（canGoBack = webview 自身是否有历史） */
export interface BackPressPayload {
  canGoBack: boolean
}

/** 返回 `true` 表示本次返回已被消费 */
export type BackPressHandler = (payload: BackPressPayload) => boolean | void

/** 应用层处理器（context.ui.onBackPressed 注册，按注册顺序） */
const appHandlers = new Set<BackPressHandler>()
/** 平台层处理器（宿主壳注册，后注册者优先——将来若有嵌套壳层，内层先拿到机会） */
const hostHandlers: BackPressHandler[] = []

let unregisterNative: (() => void) | null = null
/** 原生监听注册中（异步）：期间又出现订阅者时避免重复注册 */
let registering = false

function hasSubscribers(): boolean {
  return appHandlers.size > 0 || hostHandlers.length > 0
}

/** 摘除原生监听（幂等） */
function releaseNative(): void {
  const unregister = unregisterNative
  unregisterNative = null
  if (!unregister) return
  try {
    unregister()
  } catch (e) {
    // 摘除失败只影响「恢复系统默认返回」，不阻断订阅方逻辑，但必须可观测
    logger.warn('[BackButton] native listener unregister failed', e)
  }
}

/** 确保原生监听在场（无订阅者时不再注册） */
async function ensureNative(): Promise<void> {
  if (unregisterNative || registering) return
  registering = true
  try {
    const { onBackButtonPress } = await import('@tauri-apps/api/app')
    const listener = await onBackButtonPress(dispatchBackPress)
    // 注册期间订阅者可能已全部退订（异步窗口）：此时立即摘除，避免监听泄漏
    if (!hasSubscribers()) {
      void listener.unregister()
      return
    }
    unregisterNative = () => void listener.unregister()
  } catch {
    // 非 Tauri 环境（dev-shell 浏览器 / 单元测试）无该 API：降级为永不触发
    unregisterNative = null
  } finally {
    registering = false
  }
}

/**
 * 分发一次系统返回
 *
 * 顺序：应用层（注册顺序）→ 平台层（后注册优先）。任一处返回 true 即终止分发，
 * 使「谁处理谁负责」显式化——不返回 true 的处理器不能吞掉返回。
 */
export function dispatchBackPress(payload: BackPressPayload): void {
  for (const handler of appHandlers) {
    if (invokeHandler(handler, payload, 'app')) return
  }
  for (let i = hostHandlers.length - 1; i >= 0; i -= 1) {
    if (invokeHandler(hostHandlers[i], payload, 'host')) return
  }
}

/** 调用单个处理器：异常隔离（一个处理器抛错不该让返回整体失效），结果归一为布尔 */
function invokeHandler(
  handler: BackPressHandler,
  payload: BackPressPayload,
  layer: 'app' | 'host',
): boolean {
  try {
    return handler(payload) === true
  } catch (e) {
    logger.error(`[BackButton] ${layer} back handler threw`, e)
    return false
  }
}

function subscribe(store: Set<BackPressHandler>, handler: BackPressHandler): () => void {
  store.add(handler)
  void ensureNative()
  return () => {
    store.delete(handler)
    // 没有订阅者了：摘除原生监听，恢复系统默认返回行为
    if (!hasSubscribers()) releaseNative()
  }
}

/** 订阅应用层返回处理（`context.ui.onBackPressed` 的机制实现） */
export function subscribeAppBackPressed(handler: BackPressHandler): () => void {
  return subscribe(appHandlers, handler)
}

/**
 * 订阅平台层返回处理（宿主壳兜底）
 *
 * 宿主壳只在「覆盖层打开 / 屏栈可回退 / 不在主屏」时订阅——它一旦常驻，系统
 * 默认的「主屏返回即退出应用」就永远不会发生。
 */
export function subscribeHostBackPressed(handler: BackPressHandler): () => void {
  hostHandlers.push(handler)
  void ensureNative()
  return () => {
    const index = hostHandlers.indexOf(handler)
    if (index >= 0) hostHandlers.splice(index, 1)
    if (!hasSubscribers()) releaseNative()
  }
}
