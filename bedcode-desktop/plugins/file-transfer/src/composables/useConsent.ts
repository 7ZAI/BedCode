/**
 * 首连确认编排 — 队列 + 30s 倒计时 + 应答命令路由（spec 决策 6）
 *
 * 订阅插件事件 `plugin:file-transfer:consent-requested`（宿主 peer:consent
 * topic 经 WASM 代理原样透传的 camelCase 契约），维护单请求弹窗队列：同一
 * 时间至多一个待确认项，后续请求排队；30s 倒计时归零按拒绝先行结算释放闸门
 * （与宿主 sweeper CONFIRM_TIMEOUT 同值，拨入方立即收到 Denied）。应答经
 * `file-transfer.respond-consent` 命令回流并携带 requestId；超时后迟到的
 * 应答未命中任何待确认项时静默无害（命令面返回 hit:false）。
 *
 * 状态为模块级单例且编排在插件激活期常驻（index.ts activate 时 start、
 * deactivate 时 stop），不依赖视图挂载——弹窗渲染在 FileTransferView 内，
 * 用户不在面板时经状态栏项跳转后再处理（已知体验折衷，spec 决策 6）。
 */
import { ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'

/** 插件 consent-requested 事件载荷（宿主 camelCase 契约形状） */
export interface ConsentRequest {
  requestId: string
  /** 完整节点 ID（指纹核对依据） */
  nodeId: string
  /** 短指纹（前 8 位；无名设备时的展示兜底） */
  fingerprintShort: string
  /** 设备名（发现缓存解析；离线/缺失为 null） */
  deviceName: string | null
}

/** 弹窗超时：与宿主 crate transport::CONFIRM_TIMEOUT 保持一致 */
export const CONSENT_TIMEOUT_MS = 30_000

/**
 * 展示名兜底（纯函数）：有设备名用名，无名回退短指纹（再兜底截取完整 ID）
 */
export function consentDisplayName(request: ConsentRequest): string {
  if (request.deviceName) return request.deviceName
  return request.fingerprintShort || request.nodeId.slice(0, 8)
}

/** 载荷校验与归一化（纯函数）：畸形事件丢弃而非半途崩溃 */
function parseRequest(payload: unknown): ConsentRequest | null {
  if (!payload || typeof payload !== 'object') return null
  const p = payload as Record<string, unknown>
  if (typeof p.requestId !== 'string' || p.requestId === '') return null
  if (typeof p.nodeId !== 'string' || p.nodeId === '') return null
  return {
    requestId: p.requestId,
    nodeId: p.nodeId,
    fingerprintShort:
      typeof p.fingerprintShort === 'string' && p.fingerprintShort !== ''
        ? p.fingerprintShort
        : p.nodeId.slice(0, 8),
    deviceName: typeof p.deviceName === 'string' && p.deviceName !== '' ? p.deviceName : null,
  }
}

// ==================== 模块级共享状态（跨组件单例） ====================

/** 当前弹窗展示的确认请求；null = 无弹窗 */
const currentRequest = ref<ConsentRequest | null>(null)
/** 剩余秒数（弹窗倒计时展示；结算由 settleTimer 精确触发） */
const remainingSeconds = ref(CONSENT_TIMEOUT_MS / 1000)
/** 待确认总数（当前展示 + 排队；状态栏项计数源） */
const pendingCount = ref(0)

let queue: ConsentRequest[] = []
/** 已受理过的 requestId（去重：事件重复到达幂等；会话级小集合无需淘汰） */
const seenIds = new Set<string>()
let boundContext: PluginContext | null = null
let subscription: Disposable | null = null
let settleTimer: ReturnType<typeof setTimeout> | null = null
let tickTimer: ReturnType<typeof setInterval> | null = null
let started = false

function syncPendingCount(): void {
  pendingCount.value = (currentRequest.value ? 1 : 0) + queue.length
}

function clearTimers(): void {
  if (settleTimer) {
    clearTimeout(settleTimer)
    settleTimer = null
  }
  if (tickTimer) {
    clearInterval(tickTimer)
    tickTimer = null
  }
}

async function respond(requestId: string, accepted: boolean): Promise<void> {
  if (!boundContext) return
  try {
    // 迟到应答（宿主已超时回收）返回 hit:false，静默无害
    await boundContext.commands.execute('file-transfer.respond-consent', { requestId, accepted })
  } catch (e) {
    console.error('[File Transfer] respond-consent failed:', e)
  }
}

/** 展示待确认项并启动倒计时（调用方保证闸门空闲） */
function present(next: ConsentRequest): void {
  currentRequest.value = next
  syncPendingCount()
  remainingSeconds.value = Math.round(CONSENT_TIMEOUT_MS / 1000)
  clearTimers()
  // 结算定时器独立于展示 tick：精确对齐 30s，避免逐秒累加漂移错过宿主 sweeper
  settleTimer = setTimeout(() => {
    void answer(false)
  }, CONSENT_TIMEOUT_MS)
  tickTimer = setInterval(() => {
    if (remainingSeconds.value > 0) remainingSeconds.value--
  }, 1000)
}

function showNext(): void {
  const next = queue.shift()
  if (next) present(next)
}

/** 应答当前请求并推进队列；current 为空（含迟到应答）时静默无害 */
async function answer(accepted: boolean): Promise<void> {
  const request = currentRequest.value
  if (!request) return
  clearTimers()
  currentRequest.value = null
  syncPendingCount()
  await respond(request.requestId, accepted)
  showNext()
}

function handleRequested(payload: unknown): void {
  const request = parseRequest(payload)
  if (!request) return
  // 重复事件幂等：同一 requestId 不二次入队/弹窗
  if (seenIds.has(request.requestId)) return
  seenIds.add(request.requestId)

  // 单请求闸门：已有弹窗或排队时入队，保证用户一次只面对一个确认
  if (currentRequest.value || queue.length > 0) {
    queue.push(request)
    syncPendingCount()
    return
  }
  present(request)
}

// ==================== 控制器 ====================

export interface ConsentController {
  currentRequest: Ref<ConsentRequest | null>
  remainingSeconds: Ref<number>
  pendingCount: Ref<number>
  /** 幂等启动：激活期常驻订阅（index.ts activate 调用；重复调用不叠加） */
  start(): void
  /** 停止订阅并复位全部状态（deactivate 对称清理） */
  stop(): void
  /** 接受当前请求并推进队列 */
  accept(): Promise<void>
  /** 拒绝当前请求并推进队列（关闭弹窗等同拒绝） */
  deny(): Promise<void>
}

/**
 * 首连确认控制器：index.ts 与视图组件共用同一单例状态。
 * context 以最近一次调用为准（deactivate 后重激活换新 context 时重新绑定）。
 */
export function useConsent(context: PluginContext): ConsentController {
  return {
    currentRequest,
    remainingSeconds,
    pendingCount,
    start() {
      boundContext = context
      if (started) return
      started = true
      subscription = context.events.on('plugin:file-transfer:consent-requested', handleRequested)
    },
    stop() {
      subscription?.dispose()
      subscription = null
      started = false
      clearTimers()
      currentRequest.value = null
      queue = []
      seenIds.clear()
      remainingSeconds.value = CONSENT_TIMEOUT_MS / 1000
      syncPendingCount()
    },
    accept() {
      return answer(true)
    },
    deny() {
      return answer(false)
    },
  }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetConsentForTest(): void {
  clearTimers()
  currentRequest.value = null
  queue = []
  seenIds.clear()
  remainingSeconds.value = CONSENT_TIMEOUT_MS / 1000
  syncPendingCount()
  subscription?.dispose()
  subscription = null
  boundContext = null
  started = false
}
