/**
 * 远端目录浏览 (Mobile) — host-peer 契约版
 *
 * 两级结构：对端共享根清单（peer_list_shared_roots）→ 根内目录树
 * （browse_directory(dirId, relPath)）。path 语义 = 「根内相对路径」，
 * dirId 为共享根条目 id（browse/pull 按其寻址）。
 */
import { ref, computed } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import type { RemoteEntry } from '../types'

/** 共享根条目 */
export interface SharedRootRef {
  id: string
  name: string
}

export function useRemoteFs(context: PluginContext) {
  /** 当前所在共享根（null = 处于根清单层） */
  const currentRoot = ref<SharedRootRef | null>(null)
  /** 根内相对路径（"" = 根目录） */
  const currentPath = ref('')
  /** 目录项列表（根清单层复用同一展示容器，isDir=true 视觉呈现为可进入） */
  const entries = ref<RemoteEntry[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  /** 对端分区存储过滤提示 */
  const notice = ref<string | null>(null)

  /** 已勾选文件名集合（当前目录内唯一） */
  const selected = ref<Set<string>>(new Set())

  /** 面包屑分段：根名 + 路径段 */
  const crumbs = computed(() => {
    if (!currentRoot.value) return []
    return [currentRoot.value.name, ...currentPath.value.split('/').filter(Boolean)]
  })

  const selectedCount = computed(() => selected.value.size)

  const selectedTotalSize = computed(() => {
    let total = 0
    for (const e of entries.value) {
      if (selected.value.has(e.name) && !e.isDir) total += e.size
    }
    return total
  })

  const allSelected = computed(
    () => entries.value.length > 0 && entries.value.every((e) => selected.value.has(e.name)),
  )

  /** 加载共享根清单（path="" 且未选根） */
  async function loadRoots(): Promise<void> {
    loading.value = true
    error.value = null
    try {
      const data = await context.commands.execute('file-transfer.list-remote', {
        path: '',
        dirId: '',
      })
      const roots: SharedRootRef[] = Array.isArray(data?.roots) ? data.roots : []
      entries.value = roots.map((r) => ({ name: r.name, size: 0, mtime: 0, isDir: true }))
      notice.value = null
      currentPath.value = ''
    } catch (e) {
      console.error('[File Transfer] list-remote roots FAILED:', e)
      error.value = context.i18n.t('transfer.table.dirUnavailable')
      entries.value = []
    } finally {
      loading.value = false
    }
  }

  /** 加载当前根内的 relPath 目录 */
  async function loadDir(relPath: string): Promise<void> {
    if (!currentRoot.value) return
    loading.value = true
    error.value = null
    try {
      const data = await context.commands.execute('file-transfer.list-remote', {
        path: relPath,
        dirId: currentRoot.value.id,
      })
      entries.value = Array.isArray(data?.entries) ? data.entries : []
      notice.value = data?.notice ?? null
      currentPath.value = relPath
      selected.value = new Set()
    } catch (e) {
      console.error(`[File Transfer] list-remote FAILED: path='${relPath}'`, e)
      error.value = context.i18n.t('transfer.table.dirUnavailable')
      entries.value = []
    } finally {
      loading.value = false
    }
  }

  /** 刷新当前层级（根清单或目录） */
  async function refresh(): Promise<void> {
    if (currentRoot.value) await loadDir(currentPath.value)
    else await loadRoots()
  }

  /** 进入共享根 */
  async function enterRoot(root: SharedRootRef): Promise<void> {
    currentRoot.value = root
    await loadDir('')
  }

  /** 进入子目录（仅根内有效） */
  async function cd(name: string): Promise<void> {
    if (!currentRoot.value) {
      // 根清单层点击 = 进入该共享根（name 即根展示名）
      const root = { id: name, name }
      await enterRoot(root)
      return
    }
    const next = currentPath.value ? `${currentPath.value}/${name}` : name
    await loadDir(next)
  }

  /** 返回上级（根内首层返回 → 回根清单） */
  async function up(): Promise<void> {
    if (!currentRoot.value) return
    const segs = currentPath.value.split('/').filter(Boolean)
    segs.pop()
    if (segs.length === 0) {
      await goRoot()
    } else {
      await loadDir(segs.join('/'))
    }
  }

  /** 回到共享根清单 */
  async function goRoot(): Promise<void> {
    currentRoot.value = null
    await loadRoots()
  }

  /** 面包屑跳转（0 = 根清单；i≥1 = 根内路径段） */
  async function goTo(index: number): Promise<void> {
    if (index < 0 || !currentRoot.value) {
      await goRoot()
      return
    }
    const segs = currentPath.value.split('/').filter(Boolean).slice(0, index)
    await loadDir(segs.join('/'))
  }

  function toggle(name: string): void {
    const next = new Set(selected.value)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    selected.value = next
  }

  function toggleAll(): void {
    if (!currentRoot.value) return
    if (allSelected.value) selected.value = new Set()
    else selected.value = new Set(entries.value.map((e) => e.name))
  }

  function clearSelection(): void {
    selected.value = new Set()
  }

  /** 重置浏览状态（对端下线时调用） */
  function reset(): void {
    currentRoot.value = null
    currentPath.value = ''
    entries.value = []
    loading.value = false
    error.value = null
    notice.value = null
    selected.value = new Set()
  }

  /** 当前勾选文件名列表（不含目录） */
  const selectedFiles = computed(() =>
    entries.value.filter((e) => !e.isDir && selected.value.has(e.name)).map((e) => e.name),
  )

  return {
    currentRoot,
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
    refresh,
    enterRoot,
    cd,
    up,
    goRoot,
    goTo,
    toggle,
    toggleAll,
    clearSelection,
    reset,
    loadRoots,
  }
}
