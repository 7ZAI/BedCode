/**
 * File Transfer 插件 mock（dev-shell 专用）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册的
 * handler。本模块为 com.bedcode.file-transfer 注册全部命令 handler，并模拟
 * 事件推送（devices-changed / connection-changed / tasks-changed 等），使插件在
 * dev-shell 中展示「有数据」的完整形态，便于 UI 评审与样式调试。
 *
 * 对等领域的种子数据由插件工程持有（入口导出 devMock，SDK PluginDevMock 协议），
 * 本模块只做通用接线：消费 getDevMock(pluginId).peer 种子驱动命令返回值与事件。
 * 仅在 loader 按 pluginId 匹配时注入，不污染生产宿主。
 */
import { emitDevEvent } from './session'
import { getDevMock } from '../registry'
import type { PeerDevMock, PluginContext } from '../../../src/types'

// ==================== 模拟状态 ====================

interface MockTask {
  id: string
  direction: 'download' | 'upload'
  peer: { device_id: string; name: string }
  remote_path: string
  local_path: string
  size: number
  offset: number
  state: string
  reason: string | null
  created_at: number
  updated_at: number
}

// 种子派生状态：loader 静态 import 本模块，模块求值早于 loadAll() 的 registerDevMock
// 调用，顶层读取种子恒为 undefined（dev-shell bug：面板全空态）。改为延迟初始化——
// registerFileTransferMock 注入时才读种子，与移动端 loader 传参同构
let peerSeed: PeerDevMock | undefined
let devices: PeerDevMock['devices'] = []
const connectedNodes = new Set<string>()
let activePeerId = ''
let dialBehavior: NonNullable<PeerDevMock['dialBehavior']> = {}
let dialLatencyMs = 800
/** 首连确认请求种子（缺省不演示；多条可演示排队） */
let consentRequests: NonNullable<PeerDevMock['consent']> = []

/** 可信对端种子条目（插件 devMock 导出的本地扩展字段，SDK 协议未收录） */
interface MockTrustedSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  addedAt: string
}

/** 可信对端种子（ticket 05）：可变副本驱动撤销全流程演示；缺省/异形回退空列表 */
let trustedPeers: MockTrustedSeed[] = []

/** 活跃对端节点 id（远端文件树/任务种子以其为主机演示） */
let primaryNodeId = ''
/** 拨号成功演示机节点 id（devMock.dialBehavior 中首个 connected） */


const settings = {
  // roots 为宿主 RootItem DTO（{id, name}），插件按 name 展示、按 id 寻址移除
  roots: [
    { id: 'mock-root-1', name: 'C:\\Users\\binblink\\Desktop\\共享文件夹' },
    { id: 'mock-root-2', name: 'E:\\媒体库\\相机导入' },
  ],
  download_dir: 'C:\\Users\\binblink\\Downloads\\BedCode',
  concurrency: 3,
}

// 远端共享根（对端设备侧共享目录演示；dirId + 根内相对路径寻址，
// 契约见插件 useRemoteFs.loadRoots/loadDir）
const remoteRoots = [
  { id: 'root-dcim', name: 'DCIM' },
  { id: 'root-download', name: 'Download' },
  { id: 'root-weixin', name: '微信文件' },
  { id: 'root-docs', name: '工作文档' },
]

let remoteFs: Record<
  string,
  Array<{ name: string; size: number; mtime: number; isDir: boolean }>
> = {}

function buildRemoteFs(): Record<
  string,
  Array<{ name: string; size: number; mtime: number; isDir: boolean }>
> {
  return {
    'root-dcim::': [
      { name: 'Camera', size: 0, mtime: 1754688000, isDir: true },
      { name: 'Screenshots', size: 0, mtime: 1754662000, isDir: true },
      { name: 'IMG_20240801_1932.jpg', size: 4869382, mtime: 1754664000, isDir: false },
      { name: 'IMG_20240802_0815.jpg', size: 5124300, mtime: 1754676000, isDir: false },
      { name: 'VID_20240801_1820.mp4', size: 89244416, mtime: 1754665000, isDir: false },
    ],
    'root-dcim::Camera': [
      { name: 'IMG_20240801_1800.jpg', size: 4123400, mtime: 1754664000, isDir: false },
      { name: 'IMG_20240801_1815.jpg', size: 3891100, mtime: 1754664600, isDir: false },
    ],
    'root-dcim::Screenshots': [
      { name: 'Screenshot_20240802_1015.png', size: 1843200, mtime: 1754700900, isDir: false },
      { name: 'Screenshot_20240802_1432.png', size: 2210400, mtime: 1754716300, isDir: false },
    ],
    'root-download::': [
      { name: 'apk-backup', size: 0, mtime: 1754690000, isDir: true },
      { name: 'BedCode-2.0.0.apk', size: 68_000_000, mtime: 1754560000, isDir: false },
      { name: 'Ubuntu-24.04.iso', size: 4_720_000_000, mtime: 1754550000, isDir: false },
      { name: 'Backup_2024-08.tar.gz', size: 4127191040, mtime: 1754694000, isDir: false },
    ],
    'root-weixin::': [
      { name: '产品需求文档_v3.docx', size: 248320, mtime: 1754577000, isDir: false },
      { name: '销售数据汇总.xlsx', size: 96_000, mtime: 1754570000, isDir: false },
      { name: '会议录音_产品周会.mp3', size: 12695376, mtime: 1754520000, isDir: false },
      { name: '4K测试视频_8分钟.mp4', size: 1258291200, mtime: 1754598000, isDir: false },
      { name: '4K蓝光_星际穿越.mkv', size: 4_100_000_000, mtime: 1754600000, isDir: false },
    ],
    'root-docs::': [
      { name: '产品说明书.pdf', size: 8_600_000, mtime: 1754580000, isDir: false },
      { name: '毕业设计答辩.pptx', size: 18677760, mtime: 1754512000, isDir: false },
      { name: '2024年度旅行相册.zip', size: 2470476800, mtime: 1754628000, isDir: false },
      { name: '系统更新日志.txt', size: 15240, mtime: 1754640000, isDir: false },
      { name: 'main.ts', size: 12_480, mtime: 1754540000, isDir: false },
    ],
  }
}

function fsEntries(dirId: string, path: string): any[] {
  const key = `${dirId}::${path}`
  return remoteFs[key] ?? (path === '' ? [] : (remoteFs[`${dirId}::`] ?? []))
}

// 任务快照（含全部 8 态，覆盖四色体系）；条目依赖种子派生 primaryNodeId，随 initPeerState 重建
let taskSeq = 0
function newTask(partial: Partial<MockTask>): MockTask {
  return {
    id: `mock-task-${++taskSeq}`,
    direction: 'download',
    peer: { device_id: primaryNodeId, name: '小米 14 Pro' },
    remote_path: '',
    local_path: '',
    size: 0,
    offset: 0,
    state: 'queued',
    reason: null,
    created_at: Math.floor(Date.now() / 1000) - 600,
    updated_at: Math.floor(Date.now() / 1000),
    ...partial,
  }
}

let tasks: MockTask[] = []

function buildTasks(): MockTask[] {
  taskSeq = 0
  return [
  newTask({
    id: 'mock-task-1',
    direction: 'download',
    remote_path: 'DCIM/VID_20240801_1820.mp4',
    size: 89244416,
    offset: 41933507, // 47%
    state: 'transferring',
  }),
  newTask({
    id: 'mock-task-2',
    direction: 'upload',
    remote_path: '工作文档/产品需求文档_v3.docx',
    local_path: 'C:\\workspace\\产品需求文档_v3.docx',
    size: 248320,
    offset: 248320,
    state: 'completed',
  }),
  newTask({
    id: 'mock-task-3',
    direction: 'download',
    remote_path: '2024年度旅行相册.zip',
    size: 2470476800,
    offset: 864667000, // 35%：暂停任务保留已下载进度，与排队（0%）区分
    state: 'paused',
  }),
  newTask({
    id: 'mock-task-4',
    direction: 'upload',
    remote_path: 'IMG_20240802_0815.jpg',
    local_path: 'D:\\photos\\IMG_20240802_0815.jpg',
    size: 5124300,
    offset: 1024860,
    state: 'transferring',
  }),
  newTask({
    id: 'mock-task-5',
    direction: 'download',
    remote_path: '4K测试视频_8分钟.mp4',
    size: 1258291200,
    offset: 0,
    state: 'queued',
  }),
  newTask({
    id: 'mock-task-6',
    direction: 'upload',
    remote_path: '毕业设计答辩.pptx',
    local_path: 'D:\\slides\\毕业设计答辩.pptx',
    size: 18677760,
    offset: 0,
    state: 'failed',
    reason: 'duplicate-name',
  }),
  newTask({
    id: 'mock-task-7',
    direction: 'download',
    remote_path: '会议录音_产品周会.mp3',
    size: 12695376,
    offset: 12695376,
    state: 'completed',
  }),
  newTask({
    id: 'mock-task-8',
    direction: 'upload',
    remote_path: 'Backup_2024-08.tar.gz',
    local_path: 'E:\\backup\\Backup_2024-08.tar.gz',
    size: 4127191040,
    offset: 0,
    state: 'rejected',
    reason: 'duplicate-name',
  }),
  ]
}

/** 注入时初始化种子派生状态（时序缘由见文件头种子状态块注释） */
function initPeerState(): void {
  peerSeed = getDevMock('com.bedcode.file-transfer')?.peer
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
  remoteFs = buildRemoteFs()
  tasks = buildTasks()
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
    emitDevEvent('device-connected', {
      device_id: primaryNodeId,
      device_name: '小米 14 Pro',
    })
    emitDevEvent('filesrv:peer_changed', { peerId: activePeerId, online: true })
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
  // 可信对端列表/撤销（ticket 05）：撤销从 mock 数组摘除，重进设置页可见最新列表
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

/** 定时器句柄（setInterval 返回 number，setTimeout 亦为 number） */
const timers: number[] = []

export function registerFileTransferMock(context: PluginContext): void {
  initPeerState()
  registerCommands(context)
  // 初始事件：发现快照 + 连接态 + WS 控制面在线（connOnline pill）+ 任务快照
  pushPeerSnapshot()
  emitDevEvent('device-connected', {
    device_id: primaryNodeId,
    device_name: '小米 14 Pro',
  })
  emitDevEvent('filesrv:peer_changed', { peerId: activePeerId, online: true })
  pushSnapshot()
  timers.push(startProgressSimulation())

  // 延迟补发：usePeerDevices 的订阅在组件挂载后才建立，立即注入时事件已错失
  // （dev-shell 事件总线不重放历史），与既有 mock 的延迟富化策略一致
  timers.push(
    setTimeout(() => {
      pushPeerSnapshot()
      emitDevEvent('device-connected', {
        device_id: primaryNodeId,
        device_name: '小米 14 Pro',
      })
      emitDevEvent('filesrv:peer_changed', { peerId: activePeerId, online: true })
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
}

export function disposeFileTransferMock(): void {
  while (timers.length) clearInterval(timers.pop()!)
}
