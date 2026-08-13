/**
 * 开发期 Mock 数据（仅 vite dev server 生效）
 *
 * 在 dev-shell（无真实对端 / WASM 后端）中填充页面数据，支撑 UI 开发与走查：
 * 远端文件树 / 任务队列（覆盖四色状态体系与失败原因）/ 对端在线 / 设置。
 * 真实构建（import.meta.env.DEV=false）下 MOCK_ENABLED 被静态替换为 false，
 * 整段数据随摇树移除，绝不进入产物。
 *
 * 关闭方式：URL 追加 ?mock=0，或 localStorage 置 bedcode-ft-mock=0。
 */
import type { RemoteEntry, Task, TaskDirection, TaskStateName } from './types'

export const MOCK_ENABLED: boolean =
  import.meta.env.DEV &&
  typeof window !== 'undefined' &&
  new URLSearchParams(window.location.search).get('mock') !== '0' &&
  localStorage.getItem('bedcode-ft-mock') !== '0'

/** 模拟对端（桌面开发机） */
export const MOCK_PEER = {
  id: 'desktop-mock-001',
  name: '桌面开发机',
}

/** 模拟远端文件树：key = 相对路径（'' = 根），value = 该目录下的条目 */
export const MOCK_FS_TREE: Record<string, RemoteEntry[]> = {
  '': [
    { name: '文档资料', size: 0, mtime: Date.now() - 3 * 86_400_000, isDir: true },
    { name: '照片备份', size: 0, mtime: Date.now() - 6 * 86_400_000, isDir: true },
    { name: '电影收藏', size: 0, mtime: Date.now() - 12 * 86_400_000, isDir: true },
    { name: '项目源码', size: 0, mtime: Date.now() - 20 * 86_400_000, isDir: true },
    { name: '季度报告.pptx', size: 4_400_000, mtime: Date.now() - 26 * 3_600_000, isDir: false },
    { name: '旅行记录.mp4', size: 1_450_000_000, mtime: Date.now() - 2 * 86_400_000, isDir: false },
    { name: '安装包.zip', size: 190_000_000, mtime: Date.now() - 5 * 86_400_000, isDir: false },
    { name: '系统镜像.iso', size: 4_720_000_000, mtime: Date.now() - 9 * 86_400_000, isDir: false },
  ],
  '文档资料': [
    { name: '合同扫描件.pdf', size: 8_600_000, mtime: Date.now() - 2 * 86_400_000, isDir: false },
    { name: '会议纪要.docx', size: 350_000, mtime: Date.now() - 3 * 86_400_000, isDir: false },
    { name: '产品说明书.pdf', size: 12_400_000, mtime: Date.now() - 4 * 86_400_000, isDir: false },
    { name: '发票汇总.xlsx', size: 96_000, mtime: Date.now() - 5 * 86_400_000, isDir: false },
  ],
  '照片备份': [
    { name: 'IMG_0001.jpg', size: 4_800_000, mtime: Date.now() - 8 * 86_400_000, isDir: false },
    { name: 'IMG_0002.jpg', size: 6_100_000, mtime: Date.now() - 8 * 86_400_000, isDir: false },
    { name: '全家福.png', size: 9_300_000, mtime: Date.now() - 10 * 86_400_000, isDir: false },
    { name: '2024 旅行', size: 0, mtime: Date.now() - 15 * 86_400_000, isDir: true },
  ],
  '照片备份/2024 旅行': [
    { name: 'DSC_0101.jpg', size: 5_200_000, mtime: Date.now() - 16 * 86_400_000, isDir: false },
    { name: 'DSC_0102.jpg', size: 4_700_000, mtime: Date.now() - 16 * 86_400_000, isDir: false },
    { name: 'DSC_0103.jpg', size: 6_800_000, mtime: Date.now() - 16 * 86_400_000, isDir: false },
  ],
  '电影收藏': [
    { name: '星际穿越.mkv', size: 4_100_000_000, mtime: Date.now() - 30 * 86_400_000, isDir: false },
    { name: '千与千寻.mkv', size: 1_800_000_000, mtime: Date.now() - 45 * 86_400_000, isDir: false },
    { name: '天空之城.flac', size: 28_000_000, mtime: Date.now() - 50 * 86_400_000, isDir: false },
    { name: '电影原声带.mp3', size: 12_000_000, mtime: Date.now() - 51 * 86_400_000, isDir: false },
  ],
  '项目源码': [
    { name: 'src', size: 0, mtime: Date.now() - 7 * 86_400_000, isDir: true },
    { name: 'README.md', size: 2_048, mtime: Date.now() - 7 * 86_400_000, isDir: false },
    { name: 'Cargo.toml', size: 512, mtime: Date.now() - 7 * 86_400_000, isDir: false },
  ],
  '项目源码/src': [
    { name: 'main.rs', size: 24_000, mtime: Date.now() - 7 * 86_400_000, isDir: false },
    { name: 'lib.rs', size: 18_000, mtime: Date.now() - 7 * 86_400_000, isDir: false },
    { name: 'utils.rs', size: 12_000, mtime: Date.now() - 7 * 86_400_000, isDir: false },
  ],
}

/** 构造队列 sheet 模拟任务（覆盖 传输中/排队/暂停/完成/失败 全状态） */
export function mockTasks(): Task[] {
  const now = Date.now()
  const mk = (
    id: string,
    direction: TaskDirection,
    remotePath: string,
    size: number,
    offset: number,
    state: TaskStateName,
    reason: string | null = null,
  ): Task => ({
    id,
    direction,
    peer: { deviceId: MOCK_PEER.id, name: MOCK_PEER.name },
    remotePath,
    localPath: `/storage/emulated/0/Download/${id}.part`,
    size,
    offset,
    uploadSessionId: null,
    fingerprint: { size, mtime: now - 86_400_000 },
    state,
    reason,
    initiator: 'me',
    batchId: null,
    place: null,
    createdAt: now - 3_600_000,
    updatedAt: now,
  })
  return [
    mk('mock-t1', 'download', '/电影收藏/星际穿越.mkv', 4_100_000_000, 1_230_000_000, 'transferring'),
    mk('mock-t2', 'upload', '/文档资料/会议纪要.docx', 350_000, 0, 'queued'),
    mk('mock-t3', 'download', '/旅行记录.mp4', 1_450_000_000, 760_000_000, 'paused'),
    mk('mock-t4', 'download', '/安装包.zip', 190_000_000, 190_000_000, 'completed'),
    mk('mock-t5', 'download', '/系统镜像.iso', 4_720_000_000, 1_020_000_000, 'failed', 'remote-changed'),
  ]
}
