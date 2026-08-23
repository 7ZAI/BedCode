/**
 * 远端共享目录浏览/拉取编排 — 对等网络浏览侧（issue 11）
 *
 * 数据源为宿主 browse_peer_directory 单请求会话（每次列目录对对端新拨号）；
 * 勾选文件/目录后经 pull_peer_files 一次入队——目录先在前端递归枚举为扁平
 * 文件清单（引擎仅支持单文件拉取），逐文件独立会话落本机下载目录，进度与
 * 取消复用接收任务体系（usePeerReceiving 同源任务表）。只读：无任何写操作。
 * 与 usePeerDevices 同款模块级单例模式；纯函数导出供 vitest 直测编排口径。
 */
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// ==================== 类型 ====================

/** 远端目录条目（后端 RemoteEntryDto，camelCase） */
export interface RemoteEntry {
  /** 条目名（不含路径分隔） */
  name: string
  /** 是否目录 */
  isDir: boolean
  /** 文件字节数（目录恒 0） */
  size: number
}

/** 远端目录浏览结果（后端 RemoteBrowseDto） */
export interface RemoteBrowseResult {
  entries: RemoteEntry[]
  /** 暴露端提示列表可能不全（Android 存储权限过滤，沿用既有 notice 语义） */
  filtered: boolean
}

/** 面包屑节点：显示名 + 累积相对路径（根节点 path 为空串） */
export interface RemoteCrumb {
  name: string
  path: string
}

/** 对端暴露的共享根（后端 PeerSharedRootDto；id 供寻址、name 供展示） */
export interface SharedRoot {
  id: string
  name: string
}

/** 拉取清单单项（pull_peer_files 的 files 参数） */
export interface RemotePullFile {
  relPath: string
  size: number
}

// ==================== 纯函数（vitest 直接覆盖编排口径） ====================

/** 拼接目录内相对路径（根为空串） */
export function joinRel(base: string, name: string): string {
  return base ? `${base}/${name}` : name
}

/**
 * 目录递归枚举为扁平文件清单（BFS，深度与数量双闸）
 *
 * browse 为注入的列目录函数（生产传宿主命令封装，测试传 fake）；超出深度/
 * 数量上限抛错由调用方呈现，防止误选超大目录拖垮队列。
 */
export async function enumerateDirFiles(
  browse: (relPath: string) => Promise<RemoteBrowseResult>,
  baseRel: string,
  limits: { maxDepth: number; maxFiles: number },
): Promise<RemotePullFile[]> {
  const out: RemotePullFile[] = []
  const queue: Array<{ rel: string; depth: number }> = [{ rel: baseRel, depth: 0 }]
  while (queue.length > 0) {
    const { rel, depth } = queue.shift()!
    if (depth >= limits.maxDepth) throw new Error('too-deep')
    const listing = await browse(rel)
    for (const entry of listing.entries) {
      if (entry.isDir) {
        queue.push({ rel: joinRel(rel, entry.name), depth: depth + 1 })
      } else {
        out.push({ relPath: joinRel(rel, entry.name), size: entry.size })
        if (out.length > limits.maxFiles) throw new Error('too-many-files')
      }
    }
  }
  return out
}

/** 已选条目的字节总量（目录计 0——拉取时才递归枚举出真实大小） */
export function sumSelectedBytes(entries: readonly RemoteEntry[], selectedNames: readonly string[]): number {
  return entries
    .filter((e) => selectedNames.includes(e.name) && !e.isDir)
    .reduce((sum, e) => sum + e.size, 0)
}

/** 表头全选口径：仅作用于文件（目录经下拉枚举整取，不随全选翻转） */
export function toggleAllFiles(entries: readonly RemoteEntry[], selectedNames: readonly string[]): string[] {
  const fileNames = entries.filter((e) => !e.isDir).map((e) => e.name)
  const allSelected = fileNames.length > 0 && fileNames.every((n) => selectedNames.includes(n))
  if (allSelected) {
    return selectedNames.filter((n) => !fileNames.includes(n))
  }
  return Array.from(new Set([...selectedNames, ...fileNames]))
}

/** 字节数人性化展示（与插件域 formatBytes 同口径，peers 域独立持有） */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

// ==================== 模块级共享状态（跨组件单例） ====================

const HOME_CRUMB: RemoteCrumb = { name: '', path: '' }

const nodeId = ref('')
const deviceName = ref('')
/** 对端暴露中的共享根清单（open 时一次拉取；浏览按根寻址） */
const roots = ref<SharedRoot[]>([])
/** 当前浏览的共享根（null = 尚未选择，页面呈现根清单） */
const activeRoot = ref<SharedRoot | null>(null)
const breadcrumb = ref<RemoteCrumb[]>([{ ...HOME_CRUMB }])
const entries = ref<RemoteEntry[]>([])
const loading = ref(false)
/** 目录不可用时的 i18n key（空 = 无错误） */
const errorKey = ref('')
/** 对端提示列表可能不全（Android 存储权限过滤 notice） */
const filteredNotice = ref(false)
const selectedNames = ref<string[]>([])
/** 拉取编排进行中（递归枚举 + 入队命令往返） */
const pulling = ref(false)

/** 当前目录的累积相对路径 */
const currentPath = computed(() => breadcrumb.value[breadcrumb.value.length - 1]?.path ?? '')
const hasSelection = computed(() => selectedNames.value.length > 0)
const selectedBytes = computed(() => sumSelectedBytes(entries.value, selectedNames.value))

/** 请求序号：目录快速切换时使过期响应失效，避免后发覆盖先发 */
let busySeq = 0

async function browseRel(relPath: string): Promise<RemoteBrowseResult> {
  return invoke<RemoteBrowseResult>('browse_peer_directory', {
    nodeId: nodeId.value,
    dirId: activeRoot.value?.id ?? '',
    relPath,
  })
}

/** 列举目标目录（默认当前面包屑路径；须已选定共享根） */
async function load(path?: string): Promise<void> {
  if (!activeRoot.value) return
  const target = path === undefined ? currentPath.value : path.replace(/^\/+|\/+$/g, '')
  const seq = ++busySeq
  loading.value = true
  errorKey.value = ''
  try {
    const result = await invoke<RemoteBrowseResult>('browse_peer_directory', {
      nodeId: nodeId.value,
      dirId: activeRoot.value.id,
      relPath: target,
    })
    if (seq !== busySeq) return
    entries.value = result.entries
    // 提示位只在空列表场景置位（服务端语义），非空列表一律清除
    filteredNotice.value = result.filtered && result.entries.length === 0
    // 目录内容变化后仅保留仍存在的选中项
    const alive = new Set(entries.value.map((e) => e.name))
    selectedNames.value = selectedNames.value.filter((n) => alive.has(n))
  } catch (error) {
    if (seq !== busySeq) return
    entries.value = []
    filteredNotice.value = false
    clearSelection()
    errorKey.value = 'peers.files.error.dirUnavailable'
    console.error('[PeerRemoteFiles] browse failed:', error)
  } finally {
    if (seq === busySeq) loading.value = false
  }
}

/** 进入指定共享根（重置面包屑后列其根目录；返回加载完成信号） */
function enterRoot(root: SharedRoot): Promise<void> {
  activeRoot.value = root
  breadcrumb.value = [{ ...HOME_CRUMB }]
  entries.value = []
  selectedNames.value = []
  filteredNotice.value = false
  errorKey.value = ''
  return load('')
}

/** 页面选择共享根（fire-and-forget：加载态由 loading 呈现） */
function selectRoot(root: SharedRoot): void {
  void enterRoot(root)
}

/**
 * 打开某可信对端的远端文件页：先取共享根清单——恰一个根时自动进入并等其
 * 首屏加载完成，多个根由页面呈现选择列表，零个根呈现空态。幂等重开即重取。
 */
async function open(id: string, name: string): Promise<void> {
  busySeq++
  const seq = busySeq
  nodeId.value = id
  deviceName.value = name
  roots.value = []
  activeRoot.value = null
  breadcrumb.value = [{ ...HOME_CRUMB }]
  entries.value = []
  selectedNames.value = []
  filteredNotice.value = false
  errorKey.value = ''
  loading.value = true
  let entered: Promise<void> | null = null
  try {
    const list = await invoke<SharedRoot[]>('list_peer_shared_roots', { nodeId: id })
    if (seq !== busySeq) return
    roots.value = list
    if (list.length === 1) entered = enterRoot(list[0]!)
  } catch (error) {
    if (seq !== busySeq) return
    errorKey.value = 'peers.files.error.rootsUnavailable'
    console.error('[PeerRemoteFiles] list shared roots failed:', error)
    return
  } finally {
    // 单根路径的 loading 移交 enterRoot 的 load 接管，此处不再复位
    if (!(seq === busySeq && entered)) loading.value = false
  }
  await entered
}

/** 进入子目录（压栈并列举） */
async function enterDir(entry: RemoteEntry): Promise<void> {
  if (!entry.isDir) return
  const base = currentPath.value
  breadcrumb.value = [...breadcrumb.value, { name: entry.name, path: joinRel(base, entry.name) }]
  selectedNames.value = []
  await load()
}

/** 跳转面包屑节点（截断栈并列举） */
async function navigateTo(index: number): Promise<void> {
  if (index < 0 || index >= breadcrumb.value.length) return
  breadcrumb.value = breadcrumb.value.slice(0, index + 1)
  selectedNames.value = []
  await load()
}

function refresh(): Promise<void> {
  return load()
}

function toggleSelect(name: string): void {
  selectedNames.value = selectedNames.value.includes(name)
    ? selectedNames.value.filter((n) => n !== name)
    : [...selectedNames.value, name]
}

function toggleAll(): void {
  selectedNames.value = toggleAllFiles(entries.value, selectedNames.value)
}

function clearSelection(): void {
  selectedNames.value = []
}

/**
 * 拉取已选项到本机下载目录
 *
 * 文件直取；目录先经递归枚举展开为扁平清单，合并后一次性入队（宿主逐文件
 * 独立会话顺序执行）。返回入队文件数；超限/失败抛错由调用方按 i18n key 呈现。
 */
async function pullSelection(): Promise<number> {
  if (!hasSelection.value || pulling.value) return 0
  pulling.value = true
  try {
    const files: RemotePullFile[] = []
    const dirs: RemoteEntry[] = []
    for (const name of selectedNames.value) {
      const entry = entries.value.find((e) => e.name === name)
      if (!entry) continue
      if (entry.isDir) dirs.push(entry)
      else files.push({ relPath: joinRel(currentPath.value, entry.name), size: entry.size })
    }
    for (const dir of dirs) {
      const nested = await enumerateDirFiles(browseRel, joinRel(currentPath.value, dir.name), {
        maxDepth: 16,
        maxFiles: 512,
      })
      if (files.length + nested.length > 512) throw new Error('too-many-files')
      files.push(...nested)
    }
    if (files.length === 0) return 0
    return (
      (await invoke<number>('pull_peer_files', {
        nodeId: nodeId.value,
        dirId: activeRoot.value?.id ?? '',
        files,
      })) ?? 0
    )
  } finally {
    pulling.value = false
  }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetPeerRemoteFilesForTest(): void {
  busySeq++
  nodeId.value = ''
  deviceName.value = ''
  roots.value = []
  activeRoot.value = null
  breadcrumb.value = [{ ...HOME_CRUMB }]
  entries.value = []
  loading.value = false
  errorKey.value = ''
  filteredNotice.value = false
  selectedNames.value = []
  pulling.value = false
}

export function usePeerRemoteFiles() {
  return {
    nodeId,
    deviceName,
    roots,
    activeRoot,
    breadcrumb,
    entries,
    loading,
    errorKey,
    filteredNotice,
    selectedNames,
    pulling,
    currentPath,
    hasSelection,
    selectedBytes,
    open,
    selectRoot,
    enterDir,
    navigateTo,
    refresh,
    toggleSelect,
    toggleAll,
    clearSelection,
    pullSelection,
  }
}
