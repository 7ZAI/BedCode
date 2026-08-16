/**
 * 远端目录浏览 (Mobile)
 *
 * 经 `file-transfer.list-remote` 拉取对端目录项，维护面包屑路径与多选状态。
 * 多选勾选发生在当前目录内（RemoteEntry.name 在目录内唯一），切换目录时清空。
 */
import { ref, computed } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { RemoteEntry } from '../types'
import { MOCK_ENABLED, MOCK_FS_TREE } from '../mock'

export function useRemoteFs(context: PluginContext) {
  /** 当前相对路径（相对挂载根，"" = 根目录） */
  const currentPath = ref('')
  /** 远端目录项列表 */
  const entries = ref<RemoteEntry[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  /** 对端存储权限提示（列表为空且可能被分区存储过滤时由对端服务器置位） */
  const notice = ref<string | null>(null)

  /** 已勾选文件名集合（当前目录内唯一） */
  const selected = ref<Set<string>>(new Set())

  /** 面包屑分段（不含空串） */
  const crumbs = computed(() =>
    currentPath.value.split('/').filter(Boolean),
  )

  /** 勾选项总数 */
  const selectedCount = computed(() => selected.value.size)

  /** 勾选项总大小（字节） */
  const selectedTotalSize = computed(() => {
    let total = 0
    for (const e of entries.value) {
      if (selected.value.has(e.name) && !e.isDir) total += e.size
    }
    return total
  })

  /** 是否全选当前目录文件 */
  const allSelected = computed(
    () =>
      entries.value.length > 0 &&
      entries.value.every(e => selected.value.has(e.name)),
  )

  /** 拉取指定路径的目录项 */
  async function load(path: string): Promise<void> {
    loading.value = true
    error.value = null
    try {
      // 开发期 mock：直接读本地模拟文件树（模拟 350ms 往返延迟便于观察 loading 态）
      if (MOCK_ENABLED) {
        await new Promise((r) => setTimeout(r, 350))
        entries.value = MOCK_FS_TREE[path] ?? []
        currentPath.value = path
        selected.value = new Set()
        return
      }
      const data = await context.commands.execute('file-transfer.list-remote', {
        peerId: '',
        path,
      })
      // 兼容旧对端裸数组响应（新响应为 { entries, notice }）
      entries.value = Array.isArray(data) ? data : (data?.entries ?? [])
      notice.value = Array.isArray(data) ? null : (data?.notice ?? null)
      currentPath.value = path
      selected.value = new Set()
      console.log(`[File Transfer] list-remote OK: path='${path}' entries=${entries.value.length}`)
    } catch (e) {
      console.error(`[File Transfer] list-remote FAILED: path='${path}'`, e)
      error.value = context.i18n.t('transfer.table.dirUnavailable')
      entries.value = []
    } finally {
      loading.value = false
    }
  }

  /** 进入子目录 */
  async function cd(name: string): Promise<void> {
    const next = currentPath.value
      ? `${currentPath.value}/${name}`
      : name
    await load(next)
  }

  /** 返回上级 */
  async function up(): Promise<void> {
    const segs = crumbs.value
    segs.pop()
    await load(segs.join('/'))
  }

  /** 回到根 */
  async function goRoot(): Promise<void> {
    await load('')
  }

  /** 面包屑跳转 */
  async function goTo(index: number): Promise<void> {
    const segs = crumbs.value.slice(0, index + 1)
    await load(segs.join('/'))
  }

  /** 刷新当前目录 */
  async function refresh(): Promise<void> {
    await load(currentPath.value)
  }

  /** 切换单个条目的勾选状态 */
  function toggle(name: string): void {
    const next = new Set(selected.value)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    selected.value = next
  }

  /** 全选/全不选当前目录 */
  function toggleAll(): void {
    if (allSelected.value) selected.value = new Set()
    else selected.value = new Set(entries.value.map(e => e.name))
  }

  /** 清空勾选 */
  function clearSelection(): void {
    selected.value = new Set()
  }

  /** 重置浏览状态（对端下线/切换时调用：清空残留目录条目，避免对离线对端误操作） */
  function reset(): void {
    currentPath.value = ''
    entries.value = []
    loading.value = false
    error.value = null
    notice.value = null
    selected.value = new Set()
  }

  /** 当前勾选的文件名列表（不含目录，用于入队下载） */
  const selectedFiles = computed(() =>
    entries.value.filter(e => !e.isDir && selected.value.has(e.name)).map(e => e.name),
  )

  return {
    currentPath,
    entries,
    loading,
    error,
    notice,
    crumbs,
    selected,
    selectedCount,
    selectedTotalSize,
    allSelected,
    selectedFiles,
    load,
    cd,
    up,
    goRoot,
    goTo,
    refresh,
    toggle,
    toggleAll,
    clearSelection,
    reset,
  }
}
