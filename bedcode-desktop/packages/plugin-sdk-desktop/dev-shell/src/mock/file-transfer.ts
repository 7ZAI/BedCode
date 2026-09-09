/**
 * File Transfer 插件领域 mock（dev-shell 专用，纯通用接线）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册的
 * handler。本模块注册 file-transfer 领域的命令 handler 骨架，并模拟事件推送
 * （mdns-found / connection-changed / tasks-changed 等，Phase 3 自持版 wire
 * 形状），使插件在 dev-shell 中展示「有数据」的完整形态。
 *
 * 本模块不包含任何具体业务 mock 数据：全部演示种子由插件工程持有
 * （入口导出 devMock：peer.deviceSeeds 为 mdns:found 载荷形状、transfer.tasks
 * 为引擎 PeerTransferDto camelCase 形状），注入时按 pluginId 经 getDevMock
 * 取种子驱动命令返回值与事件；未导出对应种子的插件不受影响（各子域回退空态）。
 */
import { emitDevEvent } from './session'
import { getDevMock } from '../registry'
import type { PluginContext } from '../../../src/types'

// ==================== 种子派生状态 ====================

/** 设备种子条目（插件 devMock 导出的本地扩展字段，SDK 协议未收录） */
interface DeviceSeedEntry {
  found: {
    instanceName: string
    addresses: string[]
    port: number
    txtRecords: { id: string; name?: string; ver?: string; cap?: string }
  }
  dialBehavior?: 'connected' | 'denied' | 'unreachable'
}

interface TrustedSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  addedAt: string
}

/** 对等域种子结构（插件 devMock.peer 的通用形状，dev-shell 只按需取字段） */
interface MockPeerSeed {
  connectedNodeIds?: string[]
  activeNodeId?: string
  dialLatencyMs?: number
  consent?: Array<{ requestId: string; nodeId: string; fingerprintShort: string; deviceName: string | null }>
  trusted?: unknown
  deviceSeeds?: DeviceSeedEntry[]
}

let deviceSeeds: DeviceSeedEntry[] = []
const connectedNodes = new Set<string>()
let activePeerId = ''
let dialLatencyMs = 800
let consentRequests: Array<{
  requestId: string
  nodeId: string
  fingerprintShort: string
  deviceName: string | null
}> = []
let trustedPeers: TrustedSeed[] = []

/** 任务快照（引擎 PeerTransferDto camelCase wire 形状） */
interface MockTask {
  batchId: string
  nodeId?: string
  peerName?: string
  direction: 'send' | 'receive'
  status: string
  files: Array<{ path: string; size: number }>
  totalBytes: number
  transferredBytes: number
  rateBps: number
  createdAtMs: number
  updatedAtMs: number
  detail?: string | null
  rejectReason?: string | null
  retryMeta?: Record<string, unknown>
}
let tasks: MockTask[] = []

// 共享注册表 + 设置（get-settings 返回真实 Rust 命令同构 wire）
let sharedRoots: Array<{ id: string; name: string; path: string }> = []
let downloadDir = ''
let receivingPolicy = 'ask'
let approvalTimeoutSec = 60

// 远端共享根 + 目录内容表（key = `${dirId}::${相对路径}`，契约见插件 useRemoteFs）
let remoteRoots: Array<{ id: string; name: string }> = []
let remoteFiles: Record<
  string,
  Array<{ name: string; size: number; mtime: number; isDir: boolean }>
> = {}

function fsEntries(dirId: string, path: string): any[] {
  const key = `${dirId}::${path}`
  return remoteFiles[key] ?? (path === '' ? [] : (remoteFiles[`${dirId}::`] ?? []))
}

let taskSeq = 0

function primaryName(): string {
  const nodeId = activePeerId || connectedNodes.values().next().value || ''
  const seed = deviceSeeds.find((d) => d.found.txtRecords.id === nodeId)
  return seed?.found.txtRecords.name ?? ''
}

/** 注入时初始化种子派生状态（loader 静态 import 早于 registerDevMock，注入时才读） */
function initPeerState(pluginId: string): void {
  const seed = getDevMock(pluginId)
  const peerSeed = seed?.peer as MockPeerSeed | undefined
  deviceSeeds = peerSeed?.deviceSeeds ?? []
  connectedNodes.clear()
  for (const id of peerSeed?.connectedNodeIds ?? []) connectedNodes.add(id)
  activePeerId = peerSeed?.activeNodeId ?? ''
  dialLatencyMs = peerSeed?.dialLatencyMs ?? 800
  consentRequests = peerSeed?.consent ?? []
  const raw = peerSeed?.trusted
  trustedPeers = Array.isArray(raw) ? raw.map((p) => ({ ...(p as TrustedSeed) })) : []

  const transfer = seed?.transfer as any
  try {
    // 任务种子 = 活动条目 ∪ 历史条目（引擎 store 同构：单一存储，视图按状态派生）
    const rawTasks = Array.isArray(transfer?.tasks) ? transfer.tasks : []
    const rawHistory = Array.isArray(transfer?.history) ? transfer.history : []
    tasks = JSON.parse(JSON.stringify([...rawTasks, ...rawHistory]))
  } catch {
    tasks = []
  }
  sharedRoots = transfer?.settings ? transfer.settings.roots.map((r: any) => ({ ...r })) : []
  downloadDir = transfer?.settings?.downloadDir ?? ''
  remoteRoots = transfer?.remoteFs ? transfer.remoteFs.roots.map((r: any) => ({ ...r })) : []
  remoteFiles = transfer?.remoteFs?.files ?? {}
}

// ==================== 命令 handler ====================

function registerCommands(context: PluginContext): void {
  // ==================== 设备缓存自持（Phase 3 步骤 1） ====================
  context.commands.register('file-transfer.get-device-snapshot', () => ({ devices: [] }))
  context.commands.register('file-transfer.save-device-snapshot', () => ({ ok: true }))
  // 对等拨号：入参携带显式 endpoint；按种子 dialBehavior 返回终态，
  // connected 时同步推连接态事件（错误以 rejected promise 携带字样供行内文案分流）
  context.commands.register('file-transfer.dial-peer', (args: any) => {
    const nodeId: string = args?.endpoint?.nodeId ?? args?.nodeId ?? ''
    const seed = deviceSeeds.find((d) => d.found.txtRecords.id === nodeId)
    const capable = Number.parseInt(seed?.found.txtRecords.cap ?? '0', 16) % 2 === 1
    if (!seed || !capable || connectedNodes.has(nodeId)) {
      return Promise.reject(new Error(`dial endpoint failed: peer unreachable (${nodeId})`))
    }
    const behavior = seed.dialBehavior ?? 'unreachable'
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        if (behavior === 'connected') {
          emitConnection(nodeId, true)
          resolve({ status: 'connected' })
        } else if (behavior === 'denied') {
          reject(new Error('dial endpoint failed: peer denied'))
        } else {
          reject(new Error('dial endpoint failed: peer unreachable'))
        }
      }, dialLatencyMs)
    })
  })
  context.commands.register('file-transfer.disconnect-peer', (args: any) => {
    const nodeId = args?.nodeId
    if (typeof nodeId === 'string') emitConnection(nodeId, false)
    return Promise.resolve({ existed: true })
  })
  context.commands.register('file-transfer.set-active-peer', (args: any) => {
    const id = args?.peerId
    if (typeof id === 'string') activePeerId = id
    return { ok: true }
  })

  // ==================== 首连确认 / 可信对端 ====================
  context.commands.register('file-transfer.respond-consent', () => ({ hit: true }))
  context.commands.register('file-transfer.list-trusted', () =>
    trustedPeers.map((p) => ({ ...p })),
  )
  context.commands.register('file-transfer.revoke-trusted', (args: any) => {
    const nodeId: string = args?.nodeId ?? ''
    const idx = trustedPeers.findIndex((p) => p.nodeId === nodeId)
    if (idx >= 0) trustedPeers.splice(idx, 1)
    return idx >= 0
  })

  // ==================== 任务队列（自持存储视图） ====================
  context.commands.register('file-transfer.list-tasks', () =>
    activeSendTasks().map((t) => ({ ...t })),
  )
  context.commands.register('file-transfer.list-batches', () =>
    tasks.filter((t) => t.direction === 'receive' && t.status === 'pending').map((t) => ({ ...t })),
  )
  context.commands.register('file-transfer.list-receiving', () =>
    activeReceiveTasks().map((t) => ({ ...t })),
  )
  context.commands.register('file-transfer.list-history', () =>
    historyView().map((t) => ({ ...t })),
  )
  context.commands.register('file-transfer.clear-history', () => {
    const before = tasks.length
    tasks = tasks.filter(
      (t) => !['completed', 'failed', 'rejected', 'cancelled', 'interrupted'].includes(t.status),
    )
    pushSnapshot()
    return { cleared: before - tasks.length }
  })
  context.commands.register('file-transfer.enqueue', (args: any) => {
    const paths: string[] = Array.isArray(args?.paths) ? args.paths : []
    for (const p of paths.slice(0, 3)) {
      const size = 40_000_000 + Math.round(Math.random() * 200_000_000)
      tasks.push({
        batchId: `mock-task-new-${++taskSeq}`,
        nodeId: activePeerId,
        peerName: primaryName(),
        direction: 'send',
        status: 'running',
        files: [{ path: p.split(/[\\/]/).pop() ?? p, size }],
        totalBytes: size,
        transferredBytes: 0,
        rateBps: 0,
        createdAtMs: Date.now(),
        updatedAtMs: Date.now(),
        retryMeta: { kind: 'send', paths: [p] },
      })
    }
    pushSnapshot()
    return { batchId: `mock-task-new-${taskSeq}` }
  })
  context.commands.register('file-transfer.cancel', (args: any) => {
    const t = tasks.find((x) => x.batchId === args?.taskId && x.status === 'running')
    if (t) t.status = 'cancelled'
    pushSnapshot()
    return { ok: !!t }
  })
  context.commands.register('file-transfer.retry', (args: any) => {
    const t = tasks.find(
      (x) =>
        x.batchId === args?.taskId &&
        ['failed', 'rejected', 'interrupted'].includes(x.status),
    )
    if (!t) return Promise.reject(new Error(`task not retryable: ${args?.taskId}`))
    t.status = 'running'
    t.transferredBytes = 0
    t.detail = null
    pushSnapshot()
    return Promise.resolve({ ...t })
  })

  // ==================== 接收应答 ====================
  context.commands.register('file-transfer.approve-batch', () => ({ ok: true }))
  context.commands.register('file-transfer.reject-batch', () => ({ ok: true }))
  context.commands.register('file-transfer.cancel-receiving', (args: any) => {
    const t = tasks.find((x) => x.batchId === (args?.sessionId ?? args?.batchId))
    if (t) t.status = 'cancelled'
    pushSnapshot()
    return { ok: !!t }
  })

  // ==================== 远端浏览 / 拉取 ====================
  context.commands.register('file-transfer.list-remote', (args: any) => {
    if (!args?.dirId && !args?.path) {
      return { roots: remoteRoots.map((r) => ({ ...r })) }
    }
    return { entries: fsEntries(args?.dirId ?? '', args?.path ?? '') }
  })
  context.commands.register('file-transfer.pull-files', (args: any) => {
    const names: string[] = Array.isArray(args?.files) ? args.files : []
    for (const n of names.slice(0, 2)) {
      const rel = args?.path ? `${args.path}/${n}` : n
      const meta =
        fsEntries(args?.dirId ?? '', args?.path ?? '').find((e: any) => e.name === n) ?? {}
      const size = meta.size ?? 10_000_000
      tasks.push({
        batchId: `mock-pull-${++taskSeq}`,
        nodeId: activePeerId,
        peerName: primaryName(),
        direction: 'receive',
        status: 'running',
        files: [{ path: rel, size }],
        totalBytes: size,
        transferredBytes: 0,
        rateBps: 0,
        createdAtMs: Date.now(),
        updatedAtMs: Date.now(),
        retryMeta: { kind: 'pull', dirId: args?.dirId ?? '', files: [{ relPath: rel, size }] },
      })
    }
    pushSnapshot()
    return { count: Math.min(2, names.length) }
  })

  // ==================== 设置 / 注册表 ====================
  context.commands.register('file-transfer.get-settings', () => ({
    roots: sharedRoots.map((r) => ({ ...r })),
    policy_mode: receivingPolicy === 'accept' ? 'always_accept' : receivingPolicy === 'reject' ? 'always_deny' : 'ask',
    ask_timeout_sec: approvalTimeoutSec,
    download_dir: downloadDir,
    encryption: false,
    concurrency: 1,
  }))
  context.commands.register('file-transfer.set-settings', (args: any) => {
    if (typeof args?.downloadDir === 'string') downloadDir = args.downloadDir
    if (typeof args?.receivingPolicy === 'string') receivingPolicy = args.receivingPolicy
    if (typeof args?.approvalTimeoutSec === 'number') approvalTimeoutSec = args.approvalTimeoutSec
    return { ok: true }
  })
  context.commands.register('file-transfer.mount-local', () => {
    // 浏览器无系统目录选择器：返回一条演示条目（注册表追加）
    const id = `mock-root-${sharedRoots.length + 1}`
    const entry = { id, name: `演示目录 ${sharedRoots.length + 1}`, path: `C:/demo/shared-${id}` }
    sharedRoots.push(entry)
    return { ...entry }
  })
  context.commands.register('file-transfer.update-roots', (args: any) => {
    const before = sharedRoots.length
    sharedRoots = sharedRoots.filter((r) => r.id !== args?.remove)
    return { removed: sharedRoots.length < before }
  })
  context.commands.register('file-transfer.pick-download-dir', () => ({ cancelled: true }))
  context.commands.register('file-transfer.pick-files', () => [
    'C:/demo/演示报告.pdf',
    'C:/demo/数据表.xlsx',
  ])
}

// ==================== 事件推送 ====================

/** WS 控制面在线信号（connOnline pill 展示） */
function pushControlPlaneOnline(): void {
  const nodeId = activePeerId || deviceSeeds[0]?.found.txtRecords.id || ''
  emitDevEvent('device-connected', {
    device_id: nodeId,
    device_name: primaryName(),
  })
}

/** 连接态增量：维护已连接集合并推 connection-changed（{ nodeId, connected } 契约） */
function emitConnection(nodeId: string, connected: boolean): void {
  if (connected) {
    if (connectedNodes.has(nodeId)) return
    connectedNodes.add(nodeId)
  } else {
    if (!connectedNodes.has(nodeId)) return
    connectedNodes.delete(nodeId)
  }
  emitDevEvent('plugin:file-transfer:connection-changed', { nodeId, connected })
}

function pushSnapshot(): void {
  emitDevEvent('plugin:file-transfer:tasks-changed', activeSendTasks().map((t) => ({ ...t })))
  emitDevEvent(
    'plugin:file-transfer:batches-changed',
    tasks.filter((t) => t.direction === 'receive' && t.status === 'pending').map((t) => ({ ...t })),
  )
  emitDevEvent('plugin:file-transfer:receiving-changed', activeReceiveTasks().map((t) => ({ ...t })))
  emitDevEvent('plugin:file-transfer:history-changed', historyView().map((t) => ({ ...t })))
}

/** 终态集合（与引擎 TransferEntry::is_terminal 同口径） */
const TERMINAL_STATUS = new Set(['completed', 'failed', 'rejected', 'cancelled', 'interrupted'])

/** 发送视图：仅进行中（与引擎 active_send_entries 同口径） */
function activeSendTasks(): MockTask[] {
  return tasks.filter((t) => t.direction === 'send' && !TERMINAL_STATUS.has(t.status))
}

/** 接收视图：仅 running（与引擎 active_receive_entries 同口径） */
function activeReceiveTasks(): MockTask[] {
  return tasks.filter((t) => t.direction === 'receive' && t.status === 'running')
}

/** 历史视图：终态条目按 updatedAtMs 降序（与引擎 history_view 同口径） */
function historyView(): MockTask[] {
  return tasks
    .filter((t) => TERMINAL_STATUS.has(t.status))
    .sort((a, b) => b.updatedAtMs - a.updatedAtMs)
}

/** 模拟传输中任务进度推进（每 900ms 推一次快照），返回句柄供停用清理 */
function startProgressSimulation(): number {
  return setInterval(() => {
    let changed = false
    for (const t of tasks) {
      if (t.status !== 'running') continue
      // 每 tick 前进 0.5%～1.5%，完成时置 completed
      const step = Math.round(t.totalBytes * (0.005 + Math.random() * 0.01))
      t.transferredBytes = Math.min(t.totalBytes, t.transferredBytes + step)
      t.rateBps = Math.round(step / 0.9)
      if (t.transferredBytes >= t.totalBytes) {
        t.status = 'completed'
        t.transferredBytes = t.totalBytes
        t.rateBps = 0
      }
      t.updatedAtMs = Date.now()
      changed = true
    }
    if (changed) pushSnapshot()
  }, 900)
}

// ==================== 注入入口 ====================

/**
 * 注册 file-transfer 领域命令 mock（loader 在 activate 前调用）
 *
 * @returns Disposable：清理模拟定时器（插件 deactivate 时调用）
 */
export function registerFileTransferMock(context: PluginContext, pluginId: string): {
  dispose(): void
} {
  initPeerState(pluginId)
  registerCommands(context)

  const timers: number[] = [startProgressSimulation()]

  // 设备种子逐台延迟推送 mdns-found（订阅在组件挂载后才建立，dev-shell 总线
  // 不重放历史）；已连接种子同步补发连接态与控制面在线信号
  deviceSeeds.forEach((seed, i) => {
    timers.push(
      setTimeout(() => {
        emitDevEvent('plugin:file-transfer:mdns-found', { ...seed.found })
      }, 400 + i * 300),
    )
  })
  timers.push(
    setTimeout(() => {
      for (const nodeId of connectedNodes) emitConnection(nodeId, true)
      pushControlPlaneOnline()
      pushSnapshot()
    }, 600),
  )

  // 首连确认种子延迟逐条推送（1.2s 起步、间隔 2s）：先弹第一条，后续排队——
  // 可同时演示确认弹窗与状态栏「{n} 台设备等待确认」计数两路
  for (let i = 0; i < consentRequests.length; i++) {
    const req = consentRequests[i]
    timers.push(
      setTimeout(() => {
        emitDevEvent('plugin:file-transfer:consent-requested', { ...req })
      }, 1200 + i * 2000),
    )
  }

  return {
    dispose() {
      while (timers.length) clearInterval(timers.pop()!)
    },
  }
}
