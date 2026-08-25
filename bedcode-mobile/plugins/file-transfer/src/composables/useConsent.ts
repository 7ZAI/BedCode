/**
 * 首连确认编排 — 终端配对迁移规则 + 插件对话框 API 全局确认框（spec 决策 7）
 *
 * 订阅插件事件 `plugin:file-transfer:consent-requested`（宿主 peer:consent
 * topic 经 WASM 代理原样透传的 camelCase 契约），编排在插件激活期常驻
 * （index.ts activate 时 start、deactivate 时 stop），不依赖视图挂载——
 * 确认框经 context.dialogs.showConfirm 全局弹出（宿主对话框宿主挂在 App 根，
 * 用户身处任意页面均可达）。
 *
 * 迁移规则先行：请求方设备名命中本地持久化 paired_devices 名单（终端配对
 * 记录即同一用户的证明，ADR 0002）→ 静默自动互信 + autoTrusted toast，不弹窗、
 * 不入队（即使当前有其他确认框在展示）；匹配是无头纯函数且无名记录不参与
 * （宁可多弹勿误信）。名单经 storage 读取，缺失/损坏回退空名单退化为正常弹窗。
 *
 * 未命中名单的请求走单闸门队列 + 30s 超时：超时按拒绝先行结算释放闸门
 * （与宿主 sweeper CONFIRM_TIMEOUT 同值，拨入方立即收到 Denied）；应答经
 * `file-transfer.respond-consent` 命令回流携带 requestId；超时后迟到的对话框
 * 结果未命中待确认项时静默无害。倒计时文案为静态提示——通用对话框 API 不支持
 * 内容动态刷新，精确结算仍由 settle 定时器保证。
 */
import { ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-mobile'

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

/** 确认框超时：与宿主 crate transport::CONFIRM_TIMEOUT 保持一致 */
export const CONSENT_TIMEOUT_MS = 30_000

/** 本地持久化的终端配对设备名单存储键（与宿主 localStorage 键名对齐） */
export const PAIRED_DEVICES_KEY = 'paired_devices'

/**
 * 展示名兜底（纯函数）：有设备名用名，无名回退短指纹（再兜底截取完整 ID）
 */
export function consentDisplayName(request: ConsentRequest): string {
  if (request.deviceName) return request.deviceName
  return request.fingerprintShort || request.nodeId.slice(0, 8)
}

/**
 * 迁移规则匹配（纯函数）：请求方设备名是否命中已配对终端设备名单
 *
 * 无名记录不参与匹配（宁可多弹一次窗，不可误信陌生节点）。
 */
export function matchesTerminalPairedDevice(
  deviceName: string | null,
  pairedNames: readonly string[],
): boolean {
  return !!deviceName && pairedNames.includes(deviceName)
}

/**
 * 配对名单归一化（纯函数）：容忍缺失/损坏/异形条目，回退空名单
 *
 * 条目接受纯字符串或 { name } 形状（后者兼容宿主 PairedDevice 对象直接落盘
 * 的历史形态）；其余畸形条目逐项剔除而非整体作废。非数组输入一律回退空名单
 * ——调用方退化为正常弹窗路径，不报错。
 */
export function normalizePairedNames(raw: unknown): string[] {
  if (!Array.isArray(raw)) return []
  const names: string[] = []
  for (const entry of raw) {
    if (typeof entry === 'string' && entry !== '') {
      names.push(entry)
    } else if (
      entry &&
      typeof entry === 'object' &&
      typeof (entry as Record<string, unknown>).name === 'string' &&
      (entry as Record<string, unknown>).name !== ''
    ) {
      names.push((entry as { name: string }).name)
    }
  }
  return names
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
/** 最近一次因终端配对迁移规则被自动互信的设备名（测试断言 / 观测用） */
const autoTrustedName = ref<string | null>(null)
/** 待确认总数（当前展示 + 排队） */
const pendingCount = ref(0)

let queue: ConsentRequest[] = []
/** 已受理过的 requestId（去重：事件重复到达幂等；会话级小集合无需淘汰） */
const seenIds = new Set<string>()
let boundContext: PluginContext | null = null
let subscription: Disposable | null = null
let settleTimer: ReturnType<typeof setTimeout> | null = null
/** 当前展示请求的幂等结算函数（accept/deny 编程式入口复用；无展示时为 null） */
let activeFinish: ((accepted: boolean) => Promise<void>) | null = null
let started = false

function syncPendingCount(): void {
  pendingCount.value = (currentRequest.value ? 1 : 0) + queue.length
}

function clearTimers(): void {
  if (settleTimer) {
    clearTimeout(settleTimer)
    settleTimer = null
  }
}

function clearActiveFinish(): void {
  activeFinish = null
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

/** 从插件存储读取配对名单：缺失/损坏回退空名单（退化为正常弹窗，不报错） */
async function loadPairedNames(): Promise<string[]> {
  if (!boundContext) return []
  try {
    return normalizePairedNames(await boundContext.storage.get(PAIRED_DEVICES_KEY))
  } catch (e) {
    console.error('[File Transfer] load paired devices failed:', e)
    return []
  }
}

/**
 * 展示待确认请求并等待结算（迁移规则判定已由调用方完成）
 *
 * 结算三路互斥且幂等（settled 标志）：用户信任 / 用户拒绝或关闭 / 30s 超时
 * 自动拒绝；后到的路径静默返回，保证 respond-consent 恰好发送一次。
 */
function present(next: ConsentRequest): void {
  currentRequest.value = next
  syncPendingCount()

  let settled = false
  const finish = async (accepted: boolean): Promise<void> => {
    if (settled || !started) return
    settled = true
    clearTimers()
    clearActiveFinish()
    currentRequest.value = null
    syncPendingCount()
    await respond(next.requestId, accepted)
    showNext()
  }

  // 结算定时器独立于对话框 promise：精确对齐 30s 先行结算释放宿主闸门，
  // 迟到的用户操作因 settled 已置位而静默无害
  settleTimer = setTimeout(() => {
    void finish(false)
  }, CONSENT_TIMEOUT_MS)
  activeFinish = finish

  const context = boundContext
  if (!context) return
  const displayName = consentDisplayName(next)
  const message = [
    context.i18n.t('transfer.consent.body', { name: displayName }),
    context.i18n.t('transfer.consent.fingerprint', { fingerprint: next.fingerprintShort }),
    context.i18n.t('transfer.consent.timeoutHint', { seconds: CONSENT_TIMEOUT_MS / 1000 }),
    // 无名设备身份提示：宁可多一分核对，不可误信陌生节点
    ...(next.deviceName ? [] : [context.i18n.t('transfer.consent.namelessHint')]),
  ].join('\n')

  void context.dialogs
    .showConfirm({
      title: context.i18n.t('transfer.consent.title'),
      message,
      variant: 'warning',
      confirmText: context.i18n.t('transfer.consent.trust'),
      cancelText: context.i18n.t('transfer.consent.deny'),
      // 点击背景关闭等同拒绝：不给「误触消失不结算」留口子
      dismissible: true,
    })
    .then((confirmed) => finish(confirmed === true))
    .catch(() => finish(false))
}

function showNext(): void {
  const next = queue.shift()
  if (next) present(next)
}

async function handleRequested(payload: unknown): Promise<void> {
  const request = parseRequest(payload)
  if (!request) return
  // 重复事件幂等：同一 requestId 不二次入队/弹窗
  if (seenIds.has(request.requestId)) return
  seenIds.add(request.requestId)

  // 迁移规则先行：已配对终端设备立即放行，即使当前有其他弹窗在展示
  // （不占用用户交互，也不入队等待）；名单拉取失败按「无配对」走人工确认
  const pairedNames = await loadPairedNames()
  if (matchesTerminalPairedDevice(request.deviceName, pairedNames)) {
    autoTrustedName.value = request.deviceName
    boundContext?.dialogs.showToast(
      boundContext.i18n.t('transfer.consent.autoTrustedToast', { name: request.deviceName }),
      'success',
    )
    console.info('[File Transfer] terminal-paired device auto-trusted:', request.deviceName)
    void respond(request.requestId, true)
    return
  }

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
  autoTrustedName: Ref<string | null>
  pendingCount: Ref<number>
  /** 幂等启动：激活期常驻订阅（index.ts activate 调用；重复调用不叠加） */
  start(): void
  /** 停止订阅并复位全部状态（deactivate 对称清理；在途对话框晚到结果被忽略） */
  stop(): void
  /** 编程式接受当前请求（对话框路径之外的测试/扩展入口；已结算则无害） */
  accept(): Promise<void>
  /** 编程式拒绝当前请求（已结算则无害） */
  deny(): Promise<void>
}

/**
 * 首连确认控制器：index.ts 与潜在观测组件共用同一单例状态。
 * context 以最近一次调用为准（deactivate 后重激活换新 context 时重新绑定）。
 */
export function useConsent(context: PluginContext): ConsentController {
  return {
    currentRequest,
    autoTrustedName,
    pendingCount,
    start() {
      boundContext = context
      if (started) return
      started = true
      subscription = context.events.on('plugin:file-transfer:consent-requested', (payload) => {
        void handleRequested(payload)
      })
    },
    stop() {
      subscription?.dispose()
      subscription = null
      started = false
      clearTimers()
      clearActiveFinish()
      currentRequest.value = null
      queue = []
      seenIds.clear()
      autoTrustedName.value = null
      syncPendingCount()
    },
    accept() {
      // 无展示项（含超时后迟到调用）时静默无害，与对话框晚到结果同语义
      return activeFinish ? activeFinish(true) : Promise.resolve()
    },
    deny() {
      return activeFinish ? activeFinish(false) : Promise.resolve()
    },
  }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetConsentForTest(): void {
  clearTimers()
  clearActiveFinish()
  currentRequest.value = null
  autoTrustedName.value = null
  queue = []
  seenIds.clear()
  pendingCount.value = 0
  subscription?.dispose()
  subscription = null
  boundContext = null
  started = false
}
