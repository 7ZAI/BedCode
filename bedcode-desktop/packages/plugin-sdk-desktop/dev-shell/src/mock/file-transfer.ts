/**
 * File Transfer 插件 mock（dev-shell 专用）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册的
 * handler。本模块为 com.bedcode.file-transfer 注册全部命令 handler，并模拟
 * 事件推送（peers-changed / tasks-changed / transfer:progress），使插件在
 * dev-shell 中展示「有数据」的完整形态，便于 UI 评审与样式调试。
 *
 * 仅在 loader 按 pluginId 匹配时注入，不污染生产宿主。
 */
import { emitDevEvent } from './session'
import type { PluginContext } from '../../src/types'

// ==================== 模拟状态 ====================

interface MockPeer {
  id: string
  name: string
  ip: string
}

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

const peers: MockPeer[] = [
  { id: 'phone-xiaomi', name: '小米 14 Pro', ip: '192.168.1.108' },
  { id: 'phone-pixel', name: 'Pixel 9', ip: '192.168.1.132' },
]
let activePeerId = 'phone-xiaomi'

const settings = {
  roots: ['C:\Users\binblink\Desktop\共享文件夹', 'E:\媒体库\相机导入'],
  download_dir: 'C:\\Users\\binblink\\Downloads\\BedCode',
  concurrency: 3,
}

// 远端文件树（peerId + path → entries）
const remoteFs: Record<string, Array<{ name: string; size: number; mtime: number; isDir: boolean }>> = {
  'phone-xiaomi::': [
    { name: 'DCIM', size: 0, mtime: 1754688000, isDir: true },
    { name: 'Download', size: 0, mtime: 1754712000, isDir: true },
    { name: '微信文件', size: 0, mtime: 1754634000, isDir: true },
    { name: '工作文档', size: 0, mtime: 1754556000, isDir: true },
    { name: '2024年度旅行相册.zip', size: 2470476800, mtime: 1754628000, isDir: false },
    { name: '系统更新日志.txt', size: 15240, mtime: 1754640000, isDir: false },
    { name: '产品需求文档_v3.docx', size: 248320, mtime: 1754577000, isDir: false },
    { name: '毕业设计答辩.pptx', size: 18677760, mtime: 1754512000, isDir: false },
    { name: 'IMG_20240801_1932.jpg', size: 4869382, mtime: 1754664000, isDir: false },
    { name: 'IMG_20240802_0815.jpg', size: 5124300, mtime: 1754676000, isDir: false },
    { name: '会议录音_产品周会.mp3', size: 12695376, mtime: 1754520000, isDir: false },
    { name: '4K测试视频_8分钟.mp4', size: 1258291200, mtime: 1754598000, isDir: false },
    { name: '4K蓝光_星际穿越.mkv', size: 4_100_000_000, mtime: 1754600000, isDir: false },
    { name: '产品说明书.pdf', size: 8_600_000, mtime: 1754580000, isDir: false },
    { name: '销售数据汇总.xlsx', size: 96_000, mtime: 1754570000, isDir: false },
    { name: 'BedCode-2.0.0.apk', size: 68_000_000, mtime: 1754560000, isDir: false },
    { name: 'Ubuntu-24.04.iso', size: 4_720_000_000, mtime: 1754550000, isDir: false },
    { name: 'main.ts', size: 12_480, mtime: 1754540000, isDir: false },
  ],
  'phone-xiaomi::DCIM': [
    { name: 'Camera', size: 0, mtime: 1754688000, isDir: true },
    { name: 'Screenshots', size: 0, mtime: 1754662000, isDir: true },
    { name: 'IMG_20240801_1800.jpg', size: 4123400, mtime: 1754664000, isDir: false },
    { name: 'IMG_20240801_1815.jpg', size: 3891100, mtime: 1754664600, isDir: false },
    { name: 'VID_20240801_1820.mp4', size: 89244416, mtime: 1754665000, isDir: false },
  ],
  'phone-xiaomi::DCIM::Camera': [
    { name: 'IMG_20240801_1800.jpg', size: 4123400, mtime: 1754664000, isDir: false },
    { name: 'IMG_20240801_1815.jpg', size: 3891100, mtime: 1754664600, isDir: false },
  ],
  'phone-pixel::': [
    { name: 'Pictures', size: 0, mtime: 1754700000, isDir: true },
    { name: 'Downloads', size: 0, mtime: 1754702000, isDir: true },
    { name: 'apk-backup', size: 0, mtime: 1754690000, isDir: true },
    { name: 'Backup_2024-08.tar.gz', size: 4127191040, mtime: 1754694000, isDir: false },
  ],
}

function fsEntries(peerId: string, path: string): any[] {
  const key = `${peerId}::${path}`
  return remoteFs[key] ?? (path === '' ? [] : remoteFs[`${peerId}::`] ?? [])
}

// 任务快照（含全部 8 态，覆盖四色体系）
let taskSeq = 0
function newTask(partial: Partial<MockTask>): MockTask {
  return {
    id: `mock-task-${++taskSeq}`,
    direction: 'download',
    peer: { device_id: 'phone-xiaomi', name: '小米 14 Pro' },
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

const tasks: MockTask[] = [
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

// ==================== 命令 handler ====================

function registerCommands(context: PluginContext): void {
  context.commands.register('file-transfer.list-tasks', () => ({
    tasks: tasks.map((t) => ({ ...t })),
  }))
  context.commands.register('file-transfer.list-peers', () => {
    // 设备名富化：list-peers 由 usePeer.refresh 调用，此时订阅已建立，
    // 同步补发 device-connected 填充 nameById（时序比延迟 setTimeout 可靠）
    for (const p of peers) {
      emitDevEvent('device-connected', {
        device_id: p.id,
        device_name: p.name,
        ip: p.ip,
      })
    }
    return {
      peers: peers.map((p) => ({ peerId: p.id })),
      activePeerId,
    }
  })
  context.commands.register('file-transfer.set-active-peer', (args: any) => {
    const id = args?.peerId
    if (id && peers.some((p) => p.id === id)) activePeerId = id
    emitDevEvent('plugin:file-transfer:peers-changed', {
      peerIds: peers.map((p) => p.id),
      activePeerId,
    })
    return { activePeerId }
  })
  context.commands.register('file-transfer.query-peer', () => {
    emitDevEvent('filesrv:peer_changed', { peerId: activePeerId, online: true })
    return { ok: true }
  })
  context.commands.register('file-transfer.list-remote', (args: any) => ({
    entries: fsEntries(args?.peerId, args?.path ?? ''),
  }))
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

function pushSnapshot(): void {
  emitDevEvent('plugin:file-transfer:tasks-changed', tasks.map((t) => ({ ...t })))
}

/** 模拟传输中任务进度推进（每 900ms 推一次快照 + progress 事件） */
function startProgressSimulation(): void {
  setInterval(() => {
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
  registerCommands(context)
  // 初始事件：对端列表 + 设备连接（富化设备名）+ 任务快照
  emitDevEvent('device-connected', {
    device_id: 'phone-xiaomi',
    device_name: '小米 14 Pro',
    ip: '192.168.1.108',
  })
  emitDevEvent('device-connected', {
    device_id: 'phone-pixel',
    device_name: 'Pixel 9',
    ip: '192.168.1.132',
  })
  emitDevEvent('filesrv:peer_changed', { peerId: activePeerId, online: true })
  emitDevEvent('plugin:file-transfer:peers-changed', {
    peerIds: peers.map((p) => p.id),
    activePeerId,
  })
  pushSnapshot()
  timers.push(startProgressSimulation())

  // 延迟补发设备名富化事件：usePeer 的 device-connected 订阅在组件挂载后才建立，
  // 立即注入时事件已错失（dev-shell 事件总线不重放历史）
  timers.push(
    setTimeout(() => {
      emitDevEvent('device-connected', {
        device_id: 'phone-xiaomi',
        device_name: '小米 14 Pro',
        ip: '192.168.1.108',
      })
      emitDevEvent('device-connected', {
        device_id: 'phone-pixel',
        device_name: 'Pixel 9',
        ip: '192.168.1.132',
      })
      emitDevEvent('plugin:file-transfer:peers-changed', {
        peerIds: peers.map((p) => p.id),
        activePeerId,
      })
    }, 600),
  )
}

export function disposeFileTransferMock(): void {
  while (timers.length) clearInterval(timers.pop()!)
}
