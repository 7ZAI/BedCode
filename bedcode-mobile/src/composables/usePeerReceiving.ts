/**
 * 接收侧编排 — 对等网络接收方向前端桥（issue 10，移动端镜像）
 *
 * 数据源为宿主接收任务表：start 时拉一次全量，此后监听 `peer-receive-changed`
 * 全量列表事件（宿主侧进度节流推送，前端免轮询 IPC）。询问弹窗数据取自列表
 * 内 status=pending 的最早一批；倒计时基于设置中的 askTimeoutSecs 计算，
 * 归零主动回执拒绝（宿主引擎 TTL 为权威兜底）。与桌面端同构，无落点设置
 * （移动端接收落点恒为系统下载目录语义，由宿主 MediaLanding 提升）。
 */
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { PeerTransferTask } from './usePeerTransfers'

/** 接收任务状态：pending 等待应答（issue 10 新增），其余与发送侧共用语义 */
export type PeerReceiveStatus = 'pending' | PeerTransferTask['status']

/** 接收策略模式（与宿主 peer_receive 常量逐字一致） */
export type PeerReceivePolicyMode = 'ask' | 'always_accept' | 'always_deny'

/** 单条接收任务（后端复用 PeerTransferDto，direction 恒 receive） */
export interface PeerReceiveTask extends PeerTransferTask {
  status: PeerReceiveStatus
}

/** 接收设置（后端 get_peer_receive_settings） */
export interface PeerReceiveSettings {
  policyMode: PeerReceivePolicyMode
  askTimeoutSecs: number
}

// ==================== 纯函数（vitest 直接覆盖编排口径） ====================

/** 询问截止时刻（毫秒）：批创建时刻 + 策略超时窗口 */
export function offerDeadline(createdAtMs: number, timeoutSecs: number): number {
  return createdAtMs + timeoutSecs * 1000
}

/** 距截止剩余秒数（向上取整，已过期归 0） */
export function remainingSeconds(deadlineMs: number, nowMs: number): number {
  return Math.max(0, Math.ceil((deadlineMs - nowMs) / 1000))
}

/**
 * 当前应展示的询问批：最早的 pending 批（用户一次只面对一个确认，
 * 与首连确认弹窗同款队列语义——其余批次在宿主 pending 表内等待）
 */
export function pickCurrentOffer(tasks: readonly PeerReceiveTask[]): PeerReceiveTask | null {
  const pendings = tasks.filter((t) => t.status === 'pending')
  if (pendings.length === 0) return null
  return pendings.reduce((earliest, task) =>
    task.createdAtMs < earliest.createdAtMs ? task : earliest,
  )
}

// ==================== 模块级共享状态（跨组件单例） ====================

const receivingTasks = ref<PeerReceiveTask[]>([])
const settings = ref<PeerReceiveSettings>({ policyMode: 'ask', askTimeoutSecs: 60 })
/** 当前询问剩余秒数（秒级心跳驱动；无 pending 批时为 0） */
const remainingSecs = ref(0)

let unlistenFns: UnlistenFn[] = []
let started = false
/** 已对超时自动回执的批（防止倒计时重复触发 respond） */
const autoRejected = new Set<string>()
let ticker: ReturnType<typeof setInterval> | null = null

async function refresh(): Promise<void> {
  try {
    receivingTasks.value = await invoke<PeerReceiveTask[]>('list_peer_receiving')
  } catch (error) {
    console.error('[PeerReceiving] list receiving failed:', error)
  }
}

async function loadSettings(): Promise<void> {
  try {
    settings.value = await invoke<PeerReceiveSettings>('get_peer_receive_settings')
  } catch (error) {
    console.error('[PeerReceiving] load settings failed:', error)
  }
}

/** 应答当前询问（fire-and-forget 场景由调用方决定是否等待） */
async function respond(batchId: string, accepted: boolean): Promise<boolean> {
  autoRejected.add(batchId)
  try {
    return (await invoke<boolean>('respond_peer_transfer', { batchId, accepted })) ?? false
  } catch (error) {
    console.error('[PeerReceiving] respond failed:', error)
    return false
  }
}

/** 取消接收任务：pending 视同拒绝；running 经按批取消令牌中止 */
async function cancel(batchId: string): Promise<void> {
  try {
    await invoke('cancel_peer_receiving', { batchId })
  } catch (error) {
    console.error('[PeerReceiving] cancel failed:', error)
  }
}

/** 清空接收终态记录（活跃任务不受影响）；返回清除条数 */
async function clearHistory(): Promise<number> {
  try {
    return (await invoke<number>('clear_peer_receiving_history')) ?? 0
  } catch (error) {
    console.error('[PeerReceiving] clear history failed:', error)
    return 0
  }
}

/** 更新接收策略与询问超时（持久化 + 运行中节点热生效），成功后回读 */
async function setPolicy(mode: PeerReceivePolicyMode, askTimeoutSecs: number): Promise<boolean> {
  try {
    await invoke('set_peer_receive_policy', { mode, timeoutSecs: askTimeoutSecs })
    await loadSettings()
    return true
  } catch (error) {
    console.error('[PeerReceiving] set policy failed:', error)
    return false
  }
}

/**
 * 接收控制器：应用生命周期内幂等调用一次 `start`（Layout/页面挂载时触发）
 *
 * 监听宿主全量列表事件并维护本地副本；pending 批存在期间秒级心跳同时驱动
 * 倒计时展示刷新与归零自动回执拒绝。心跳为模块单例，随应用生命周期走。
 */
async function start(): Promise<void> {
  if (started) return
  started = true
  unlistenFns.push(
    await listen<PeerReceiveTask[]>('peer-receive-changed', (event) => {
      receivingTasks.value = event.payload
    }),
  )
  await Promise.all([refresh(), loadSettings()])
  ticker = setInterval(tickOffers, 1000)
}

/** 秒级心跳：刷新剩余秒数；归零的最早 pending 批自动回执拒绝 */
function tickOffers(): void {
  const offer = pickCurrentOffer(receivingTasks.value)
  if (!offer) {
    remainingSecs.value = 0
    return
  }
  const deadline = offerDeadline(offer.createdAtMs, settings.value.askTimeoutSecs)
  remainingSecs.value = remainingSeconds(deadline, Date.now())
  if (remainingSecs.value <= 0 && !autoRejected.has(offer.batchId)) {
    console.info('[PeerReceiving] offer countdown expired, auto-rejecting:', offer.batchId)
    void respond(offer.batchId, false)
  }
}

export function usePeerReceiving() {
  /** 当前应展示的询问批（null = 无弹窗） */
  const currentOffer = computed(() => pickCurrentOffer(receivingTasks.value))

  return {
    receivingTasks,
    settings,
    currentOffer,
    remainingSecs,
    start,
    refresh,
    loadSettings,
    respond,
    cancel,
    clearHistory,
    setPolicy,
  }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetPeerReceivingForTest(): void {
  unlistenFns.forEach((fn) => fn())
  unlistenFns = []
  started = false
  receivingTasks.value = []
  settings.value = { policyMode: 'ask', askTimeoutSecs: 60 }
  remainingSecs.value = 0
  autoRejected.clear()
  if (ticker) {
    clearInterval(ticker)
    ticker = null
  }
}
