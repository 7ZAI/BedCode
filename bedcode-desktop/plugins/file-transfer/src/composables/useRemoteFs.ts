/**
 * 远端目录浏览 (Desktop) — host-peer 契约版
 *
 * 两级结构：对端共享根清单（list_shared_roots）→ 根内目录树
 * （browse_directory(dirId, relPath)）。path 语义 = 「根内相对路径」，
 * dirId 为共享根条目 id。
 */
import { ref, computed, type Ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { RemoteEntry } from '../types'

/** 共享根条目 */
export interface SharedRootRef {
  id: string
  name: string
}

/** 面包屑节点：展示名 + 累积相对路径（根节点 path 为空串） */
export interface Crumb {
  name: string
  path: string
}

export function useRemoteFs(context: PluginContext, _getPeerId: () => string) {
  /** 当前所在共享根（null = 处于根清单层） */
  const currentRoot = ref<SharedRootRef | null>(null) as Ref<SharedRootRef | null>
  /** 目录项列表（根清单层复用同一容器，isDir=true 呈现为可进入） */
  const entries = ref<RemoteEntry[]>([]) as Ref<RemoteEntry[]>
  const loading = ref(false) as Ref<boolean>
  /** 目录加载失败时的 i18n key（空 = 无错误） */
  const errorKey = ref('') as Ref<string>
  /** 对端存储权限提示 */
  const notice = ref<string | null>(null) as Ref<string | null>
  /** 根内相对路径 */
  const relPath = ref('') as Ref<string>
  /** 面包屑栈 */
  const breadcrumb = ref<Crumb[]>([
    { name: 'transfer.breadcrumb.home', path: '' },
  ]) as Ref<Crumb[]>
  /** 已选文件名集合（当前目录内） */
  const selectedNames = ref<string[]>([]) as Ref<string[]>

  const selectedEntries = computed(() =>
    entries.value.filter((e) => selectedNames.value.includes(e.name)),
  )
  const hasSelection = computed(() => selectedNames.value.length > 0)

  /** 防竞态序号：目录快速切换时旧响应作废 */
  let busySeq = 0

  /** 加载共享根清单 */
  async function loadRoots(): Promise<void> {
    const seq = ++busySeq
    loading.value = true
    errorKey.value = ''
    try {
      const data = await context.commands.execute('file-transfer.list-remote', {
        path: '',
        dirId: '',
      })
      if (seq !== busySeq) return
      const roots: SharedRootRef[] = Array.isArray(data?.roots) ? data.roots : []
      entries.value = roots.map((r) => ({ name: r.name, size: 0, mtime: 0, isDir: true }))
      notice.value = null
      currentRoot.value = null
      relPath.value = ''
      breadcrumb.value = [{ name: 'transfer.breadcrumb.home', path: '' }]
      selectedNames.value = []
    } catch (e) {
      console.error('[File Transfer] list-remote roots FAILED:', e)
      if (seq !== busySeq) return
      errorKey.value = 'transfer.table.dirUnavailable'
      entries.value = []
    } finally {
      if (seq === busySeq) loading.value = false
    }
  }

  /** 加载当前根内的 relPath 目录 */
  async function loadDir(root: SharedRootRef, path: string): Promise<void> {
    const seq = ++busySeq
    loading.value = true
    errorKey.value = ''
    try {
      const data = await context.commands.execute('file-transfer.list-remote', {
        path,
        dirId: root.id,
      })
      if (seq !== busySeq) return
      entries.value = Array.isArray(data?.entries) ? data.entries : []
      notice.value = data?.notice ?? null
      relPath.value = path
      selectedNames.value = []
    } catch (e) {
      console.error(`[File Transfer] list-remote FAILED: path='${path}'`, e)
      if (seq !== busySeq) return
      errorKey.value = 'transfer.table.dirUnavailable'
      entries.value = []
    } finally {
      if (seq === busySeq) loading.value = false
    }
  }

  /** 兼容入口：无参/空路径刷新当前层级；带路径时需先有 currentRoot */
  async function load(path?: string): Promise<void> {
    if (!currentRoot.value) {
      await loadRoots()
      return
    }
    const target =
      path === undefined ? relPath.value : path.replace(/^\/+/, '').replace(/\/+$/, '')
    // 面包屑重建
    const segs = target.split('/').filter(Boolean)
    breadcrumb.value = [
      { name: 'transfer.breadcrumb.home', path: '' },
      ...segs.map((s, i) => ({ name: s, path: segs.slice(0, i + 1).join('/') })),
    ]
    await loadDir(currentRoot.value, target)
  }

  /** 进入共享根 */
  async function enterRoot(root: SharedRootRef): Promise<void> {
    currentRoot.value = root
    breadcrumb.value = [
      { name: 'transfer.breadcrumb.home', path: '' },
      { name: root.name, path: '' },
    ]
    await loadDir(root, '')
  }

  /** 进入子目录 / 根清单层点击进入共享根 */
  async function cd(name: string): Promise<void> {
    if (!currentRoot.value) {
      const data = await context.commands.execute('file-transfer.list-remote', {
        path: '',
        dirId: '',
      })
      const roots: SharedRootRef[] = Array.isArray(data?.roots) ? data.roots : []
      const root = roots.find((r) => r.name === name) ?? { id: name, name }
      await enterRoot(root)
      return
    }
    const next = relPath.value ? `${relPath.value}/${name}` : name
    breadcrumb.value = [
      ...breadcrumb.value,
      { name, path: next },
    ]
    await loadDir(currentRoot.value, next)
  }

  /** 返回上级 */
  async function up(): Promise<void> {
    if (breadcrumb.value.length <= 1) return
    if (breadcrumb.value.length === 2) {
      await goRoot()
      return
    }
    breadcrumb.value = breadcrumb.value.slice(0, -1)
    const prev = breadcrumb.value[breadcrumb.value.length - 1]
    if (currentRoot.value) await loadDir(currentRoot.value, prev.path)
  }

  /** 回到共享根清单 */
  async function goRoot(): Promise<void> {
    await loadRoots()
  }

  /** 面包屑跳转（0 = 根清单） */
  async function goTo(index: number): Promise<void> {
    if (index <= 0 || !currentRoot.value) {
      await goRoot()
      return
    }
    const crumb = breadcrumb.value[index]
    breadcrumb.value = breadcrumb.value.slice(0, index + 1)
    await loadDir(currentRoot.value, crumb.path)
  }

  async function refresh(): Promise<void> {
    if (currentRoot.value) await loadDir(currentRoot.value, relPath.value)
    else await loadRoots()
  }

  function toggle(name: string): void {
    if (selectedNames.value.includes(name)) {
      selectedNames.value = selectedNames.value.filter((n) => n !== name)
    } else {
      selectedNames.value = [...selectedNames.value, name]
    }
  }

  /** 全选/全不选当前目录文件 */
  function toggleAll(): void {
    const files = entries.value.filter((e) => !e.isDir).map((e) => e.name)
    const allOn = files.every((n) => selectedNames.value.includes(n))
    selectedNames.value = allOn ? [] : files
  }

  function clearSelection(): void {
    selectedNames.value = []
  }

  /** 重置浏览状态（对端下线时调用） */
  function reset(): void {
    currentRoot.value = null
    entries.value = []
    loading.value = false
    errorKey.value = ''
    notice.value = null
    relPath.value = ''
    breadcrumb.value = [{ name: 'transfer.breadcrumb.home', path: '' }]
    selectedNames.value = []
  }

  return {
    currentRoot,
    entries,
    loading,
    errorKey,
    notice,
    breadcrumb,
    selectedNames,
    selectedEntries,
    hasSelection,
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
    load,
  }
}
