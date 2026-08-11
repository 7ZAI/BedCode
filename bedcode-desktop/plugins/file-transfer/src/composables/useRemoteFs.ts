/**
 * 远端目录浏览
 *
 * list-remote 命令 + 面包屑栈 + 文件多选。对端是否在线由上层
 * FileTransferView 监听 peer.online 变化后触发 load()；本 composable
 * 自身不订阅事件，只负责目录状态的请求与维护。
 *
 * 选择策略：仅文件可被多选（目录经双击进入浏览），下载命令按文件入队，
 * 目录整体递归下载不在 WASM 命令契约内。
 */
import { ref, computed, type Ref } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import type { RemoteEntry } from '../types'

/** 面包屑节点：显示名 + 累积路径（根节点 path 为空串） */
export interface Crumb {
  name: string
  path: string
}

export function useRemoteFs(context: PluginContext, getPeerId: () => string) {
  /** 当前目录条目 */
  const entries = ref<RemoteEntry[]>([]) as Ref<RemoteEntry[]>
  const loading = ref(false)
  /** 目录不可用时的 i18n key（空 = 无错误） */
  const errorKey = ref('')
  /** 对端存储权限提示（列表为空且可能被分区存储过滤时由对端服务器置位） */
  const notice = ref<string | null>(null)
  /** 面包屑栈（首个为根节点） */
  const breadcrumb = ref<Crumb[]>([{ name: 'transfer.breadcrumb.home', path: '' }]) as Ref<Crumb[]>
  /** 已选文件名（相对当前目录） */
  const selectedNames = ref<string[]>([]) as Ref<string[]>

  const currentPath = computed(() => breadcrumb.value[breadcrumb.value.length - 1].path)
  const selectedEntries = computed(() =>
    entries.value.filter(e => selectedNames.value.includes(e.name)),
  )
  const hasSelection = computed(() => selectedNames.value.length > 0)

  /** 请求序号：目录快速切换时使过期响应失效，避免后发覆盖先发 */
  let busySeq = 0

  /** 归一化路径（去首尾斜杠；根为 ""） */
  function norm(path: string): string {
    return path.replace(/^\/+/, '').replace(/\/+$/, '')
  }

  /** 列举目标目录（默认当前面包屑路径） */
  async function load(path?: string): Promise<void> {
    const target = norm(path === undefined ? currentPath.value : path)
    const seq = ++busySeq
    loading.value = true
    errorKey.value = ''
    try {
      const data = await context.commands.execute('file-transfer.list-remote', {
        peerId: getPeerId(),
        path: target,
      })
      if (seq !== busySeq) return
      // 兼容旧对端裸数组响应（新响应为 { entries, notice }）
      const arr = Array.isArray(data) ? data : (data?.entries ?? [])
      notice.value = Array.isArray(data) ? null : (data?.notice ?? null)
      entries.value = arr.map((e: any) => ({
        name: e.name,
        size: e.size ?? 0,
        mtime: e.mtime ?? 0,
        isDir: !!e.isDir,
      }))
      // 目录内容变化后仅保留仍存在的选中项
      const alive = new Set(entries.value.map(e => e.name))
      selectedNames.value = selectedNames.value.filter(n => alive.has(n))
      console.log(`[File Transfer] list-remote OK: path='${target}' entries=${entries.value.length}`)
    } catch (e) {
      if (seq !== busySeq) return
      entries.value = []
      notice.value = null
      errorKey.value = 'transfer.error.dirUnavailable'
      console.error(`[File Transfer] list-remote FAILED: path='${target}'`, e)
    } finally {
      if (seq === busySeq) loading.value = false
    }
  }

  /** 进入子目录（压栈并列举） */
  async function enterDir(entry: RemoteEntry): Promise<void> {
    if (!entry.isDir) return
    const base = currentPath.value
    const path = base ? `${base}/${entry.name}` : entry.name
    breadcrumb.value = [...breadcrumb.value, { name: entry.name, path }]
    selectedNames.value = []
    await load(path)
  }

  /** 跳转面包屑节点（截断栈并列举） */
  async function navigateTo(index: number): Promise<void> {
    if (index < 0 || index >= breadcrumb.value.length) return
    breadcrumb.value = breadcrumb.value.slice(0, index + 1)
    selectedNames.value = []
    await load()
  }

  /** 切换单文件选中 */
  function toggleSelect(name: string): void {
    selectedNames.value = selectedNames.value.includes(name)
      ? selectedNames.value.filter(n => n !== name)
      : [...selectedNames.value, name]
  }

  /** 表头全选：仅作用于文件（目录不可下载） */
  function toggleAll(): void {
    const fileNames = entries.value.filter(e => !e.isDir).map(e => e.name)
    const allSelected =
      fileNames.length > 0 && fileNames.every(n => selectedNames.value.includes(n))
    selectedNames.value = allSelected
      ? selectedNames.value.filter(n => !fileNames.includes(n))
      : Array.from(new Set([...selectedNames.value, ...fileNames]))
  }

  function clearSelection(): void {
    selectedNames.value = []
  }

  /** 刷新当前目录（顶栏「刷新」按钮调用） */
  function refresh(): Promise<void> {
    return load()
  }

  /** 组件卸载时使在途请求失效 */
  function stop(): void {
    busySeq++
  }

  return {
    entries,
    loading,
    errorKey,
    notice,
    breadcrumb,
    selectedNames,
    currentPath,
    selectedEntries,
    hasSelection,
    load,
    enterDir,
    navigateTo,
    toggleSelect,
    toggleAll,
    clearSelection,
    refresh,
    stop,
  }
}
