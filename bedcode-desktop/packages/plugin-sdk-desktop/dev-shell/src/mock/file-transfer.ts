/**
 * File Transfer 插件领域 mock（dev-shell 专用，纯通用接线）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册的
 * handler。本模块注册 file-transfer 领域的命令 handler 骨架，并模拟事件推送
 * （devices-changed / connection-changed / tasks-changed 等），使插件在
 * dev-shell 中展示「有数据」的完整形态，便于 UI 评审与样式调试。
 *
 * 本模块不包含任何具体业务 mock 数据：全部演示种子由插件工程持有
 * （入口导出 devMock，SDK PluginDevMock 协议的 peer / transfer 子域），
 * 注入时按 pluginId 经 getDevMock 取种子驱动命令返回值与事件；
 * 未导出对应种子的插件不受影响（各子域回退空态）。
 */
import { emitDevEvent } from './session'
import { getDevMock } from '../registry'
import type { PluginContext, TransferTaskSeed } from '../../../src/types'

// ==================== 种子派生状态 ====================

/** 任务 DTO（宿主 wire 形状；由 TransferTaskSeed 组装） */
interface MockTask extends TransferTaskSeed {
  peer: { device_id: string; name: string }
  local_path: string
  remote_path: string
  reason: string | null
  created_at: number
  updated_at: number
}

// loader 静态 import 本模块，模块求值早于 loadAll() 的 registerDevMock 调用，
// 顶层读取种子恒为 undefined。改为注入时初始化——registerFileTransferMock
// 调用时才读种子，与移动端 loader 传参同构
let devices: Array<{ nodeId: string; deviceName: string; fileTransfer?: boolean }> = []
const connectedNodes = new Set<string>()
let activePeerId = ''
let dialBehavior: Record<string, 'connected' | 'denied' | 'unreachable'> = {}
let dialLatencyMs = 800
/** 首连确认请求种子（缺省不演示；多条可演示排队） */
let consentRequests: Array<{
  requestId: string
  nodeId: string
  fingerprintShort: string
  deviceName: string | null
}> = []

/** 可信对端种子条目（插件 devMock 导出的本地扩展字段，SDK 协议未收录） */
interface MockTrustedSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  addedAt: string
}
let trustedPeers: MockTrustedSeed[] = []

/** 活跃对端节点 id（远端文件树/任务种子以其为主机演示） */
let primaryNodeId = ''

// 共享设置（种子缺省回退空态；roots 为宿主 RootItem DTO {id, name}）
const settings = {
  roots: [] as Array<{ id: string; name: string }>,
  download_dir: '',
  concurrency: 3,
}

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

// 任务快照：由任务种子组装完整 DTO（peer 取活跃对端、时间戳取当前）
function buildTasks(seedTasks: TransferTaskSeed[]): MockTask[] {
  const nowSec = Math.floor(Date.now() / 1000)
  const primaryName = devices.find((d) => d.nodeId === primaryNodeId)?.deviceName ?? ''
  return seedTasks.map((t) => ({
    ...t,
    local_path: t.localPath ?? '',
    remote_path: t.remotePath,
    reason: t.reason ?? null,
    peer: { device_id: primaryNodeId, name: primaryName },
    created_at: nowSec - 600,
    updated_at: nowSec,
  }))
}
let tasks: MockTask[] = []

/** 注入时初始化种子派生状态（时序缘由见上） */
function initPeerState(pluginId: string): void {
  const seed = getDevMock(pluginId)
  const peerSeed = seed?.peer
  devices = peerSeed?.devices ?? []
  connectedNodes.clear()
  for (const id of peerSeed?.connectedNodeIds ?? []) connectedNodes.add(id)
  activePeerId = peerSeed?.activeNodeId ?? ''
  dialBehavior = peerSeed?.dialBehavior ?? {}
  dialLatencyMs = peerSeed?.dialLatencyMs ?? 800
  consentRequests = peerSeed?.consent ?? []
  const raw = (peerSeed as (typeof peerSeed & { trusted?: unknown }) | undefined)?.trusted
  trustedPeers = Array.isArray(raw) ? raw.map((p) => ({ ...(p as MockTrustedSeed) })) : []
  primaryNodeId = activePeerId || devices[0]?.nodeId || ''

  const transfer = seed?.transfer
  settings.roots = transfer?.settings ? transfer.settings.roots.map((r) => ({ ...r })) : []
  settings.download_dir = transfer?.settings?.downloadDir ?? ''
  settings.concurrency = transfer?.settings?.concurrency ?? 3
  remoteRoots = transfer?.remoteFs ? transfer.remoteFs.roots.map((r) => ({ ...r })) : []
  remoteFiles = transfer?.remoteFs?.files ?? {}
  tasks = buildTasks(transfer?.tasks ?? [])
}

// ==================== 命令 handler ====================

function registerCommands(context: PluginContext): void {
  context.commands.register('file-transfer.list-tasks', () => ({
    tasks: tasks.map((t) => ({ ...t })),
  }))
  context.commands.register('file-transfer.list-peers', () => ({
    // 旧契约：可传输对端列表 + 活跃对端（usePeerDevices.refresh 拉活跃态用）
    peers: devices
      .filter((d) => d.fileTransfer !== false)
      .map((d) => ({ deviceId: d.nodeId, name: d.deviceName })),
    activePeerId,
  }))
  context.commands.register('file-transfer.set-active-peer', (args: any) => {
    const id = args?.peerId
    if (typeof id === 'string' && (!id || devices.some((d) => d.nodeId === id))) {
      activePeerId = id
    }
    return { activePeerId }
  })
  // 发现快照（宿主 DiscoveredPeerDto camelCase 形状原样透传）；
  // 面板挂载晚于注入时事件已错失（总线不重放），借刷新路径补发控制面在线与全量快照
  context.commands.register('file-transfer.query-peer', () => {
    pushControlPlaneOnline()
    pushPeerSnapshot()
    return devices.map((d) => ({ ...d }))
  })
  // 对等拨号：按 devMock.dialBehavior 返回终态；connected 时同步推连接态事件
  context.commands.register('file-transfer.dial-peer', (args: any) => {
    const nodeId: string = args?.nodeId ?? ''
    const device = devices.find((d) => d.nodeId === nodeId)
    if (!device || !device.fileTransfer || connectedNodes.has(nodeId)) {
      return Promise.reject(new Error(`dial failed: cannot dial "${nodeId}"`))
    }
    const behavior = dialBehavior[nodeId] ?? 'unreachable'
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        if (behavior === 'connected') {
          emitConnection(nodeId, true)
          resolve({ status: 'connected', deviceName: device.deviceName })
        } else if (behavior === 'denied') {
          resolve({ status: 'denied', deviceName: device.deviceName })
        } else {
          reject(new Error('dial failed: node unreachable'))
        }
      }, dialLatencyMs)
    })
  })
  context.commands.register('file-transfer.disconnect-peer', (args: any) => {
    const nodeId = args?.nodeId
    if (typeof nodeId === 'string') emitConnection(nodeId, false)
    return Promise.resolve(true)
  })
  // 首连应答：命中即接受/拒绝均回执（真实宿主由 pending 表判定，超时返回 hit:false）
  context.commands.register('file-transfer.respond-consent', () => ({ hit: true }))
  // 可信对端列表/撤销：撤销从 mock 数组摘除，重进设置页可见最新列表
  context.commands.register('file-transfer.list-trusted', () =>
    trustedPeers.map((p) => ({ ...p })),
  )
  context.commands.register('file-transfer.revoke-trusted', (args: any) => {
    const nodeId: string = args?.nodeId ?? ''
    const idx = trustedPeers.findIndex((p) => p.nodeId === nodeId)
    if (idx >= 0) trustedPeers.splice(idx, 1)
    return idx >= 0
  })
  // 远端浏览（useRemoteFs 契约）：根清单层返回 { roots }，目录层返回 { entries }。
  // dirId 为空且 path 为空 = 根清单；否则按 dirId + 相对路径查表
  context.commands.register('file-transfer.list-remote', (args: any) => {
    if (!args?.dirId && !args?.path) {
      return { roots: remoteRoots.map((r) => ({ ...r })) }
    }
    return { entries: fsEntries(args?.dirId ?? '', args?.path ?? '') }
  })
  context.commands.register('file-transfer.get-settings', () => ({ ...settings }))
  context.commands.register('file-transfer.set-settings', (args: any) => {
    if (Array.isArray(args?.roots)) settings.roots = args.roots
    if (typeof args?.downloadDir === 'string') settings.download_dir = args.downloadDir
    if (typeof args?.concurrency === 'number') settings.concurrency = args.concurrency
    return { ok: true }
  })
  context.commands.register('file-transfer.set-concurrency', (args: any) => {
    settings.concurrency = args?.concurrency ?? settings.concurrency
    return { ok: true }
  })
  context.commands.register('file-transfer.enqueue', () => ({ ok: true }))
  context.commands.register('file-transfer.pause', (args: any) => {
    const t = tasks.find((x) => x.id === args?.taskId)
    if (t && t.state === 'transferring') t.state = 'paused'
    pushSnapshot()
    return { ok: true }
  })
  context.commands.register('file-transfer.resume', (args: any) => {
    const t = tasks.find((x) => x.id === args?.taskId)
    if (t && (t.state === 'paused' || t.state === 'resumable')) t.state = 'transferring'
    pushSnapshot()
    return { ok: true }
  })
  context.commands.register('file-transfer.cancel', (args: any) => {
    const t = tasks.find((x) => x.id === args?.taskId)
    if (t && t.state !== 'completed') t.state = 'cancelled'
    pushSnapshot()
    return { ok: true }
  })
  context.commands.register('file-transfer.retry', (args: any) => {
    const t = tasks.find((x) => x.id === args?.taskId)
    if (t && (t.state === 'failed' || t.state === 'rejected')) {
      t.state = 'queued'
      t.offset = 0
      t.reason = null
    }
    pushSnapshot()
    return { ok: true }
  })
  context.commands.register('file-transfer.resume-all', () => {
    for (const t of tasks) {
      if (t.state === 'paused' || t.state === 'resumable') t.state = 'transferring'
    }
    pushSnapshot()
    return { ok: true }
  })
}

// ==================== 事件推送 ====================

/** WS 控制面在线信号（connOnline pill 展示） */
function pushControlPlaneOnline(): void {
  emitDevEvent('device-connected', {
    device_id: primaryNodeId,
    device_name: devices.find((d) => d.nodeId === primaryNodeId)?.deviceName ?? '',
  })
  emitDevEvent('filesrv:peer_changed', { peerId: activePeerId, online: true })
}

/** 连接态增量：维护已连接集合并推 connection-changed（{ nodeId, connected } 契约） */
function emitConnection(nodeId: string, connected: boolean): void {
  const device = devices.find((d) => d.nodeId === nodeId)
  if (connected) {
    if (!device || connectedNodes.has(nodeId)) return
    connectedNodes.add(nodeId)
  } else {
    if (!connectedNodes.has(nodeId)) return
    connectedNodes.delete(nodeId)
  }
  emitDevEvent('plugin:file-transfer:connection-changed', {
    nodeId,
    connected,
    deviceName: device?.deviceName ?? null,
  })
}

/** 发现快照 + 连接态全量补发（dev-shell 事件总线不重放历史，晚订阅者靠它追平） */
function pushPeerSnapshot(): void {
  emitDevEvent(
    'plugin:file-transfer:devices-changed',
    devices.map((d) => ({ ...d })),
  )
  for (const nodeId of connectedNodes) {
    const device = devices.find((d) => d.nodeId === nodeId)
    emitDevEvent('plugin:file-transfer:connection-changed', {
      nodeId,
      connected: true,
      deviceName: device?.deviceName ?? null,
    })
  }
}

function pushSnapshot(): void {
  emitDevEvent(
    'plugin:file-transfer:tasks-changed',
    tasks.map((t) => ({ ...t })),
  )
}

/** 模拟传输中任务进度推进（每 900ms 推一次快照 + progress 事件），返回句柄供停用清理 */
function startProgressSimulation(): number {
  return setInterval(() => {
    let changed = false
    for (const t of tasks) {
      if (t.state !== 'transferring') continue
      // 每 tick 前进 0.5%～1.5%，完成时置 completed
      const step = Math.round(t.size * (0.005 + Math.random() * 0.01))
      t.offset = Math.min(t.size, t.offset + step)
      if (t.offset >= t.size) {
        t.state = 'completed'
        t.offset = t.size
      }
      changed = true
      emitDevEvent('plugin:transfer:progress', {
        taskId: `host-${t.id}`,
        transferred: t.offset,
        total: t.size,
        bytesPerSec: Math.round(step / 0.9),
        state: { state: t.state === 'completed' ? 'completed' : 'running' },
      })
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
  // 初始事件：发现快照 + 连接态 + WS 控制面在线（connOnline pill）+ 任务快照
  pushPeerSnapshot()
  pushControlPlaneOnline()
  pushSnapshot()
  const timers: number[] = [startProgressSimulation()]

  // 延迟补发：usePeerDevices 的订阅在组件挂载后才建立，立即注入时事件已错失
  // （dev-shell 事件总线不重放历史），与既有 mock 的延迟富化策略一致
  timers.push(
    setTimeout(() => {
      pushPeerSnapshot()
      pushControlPlaneOnline()
    }, 600),
  )

  // 首连确认种子延迟逐条推送（1.2s 起步、间隔 2s）：订阅建立后先弹第一条，
  // 后续条目进入队列——可同时演示确认弹窗与状态栏待确认计数两路
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
