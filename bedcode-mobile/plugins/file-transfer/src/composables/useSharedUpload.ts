/**
 * 共享目录上传（M1 上传页核心逻辑，M3 流直传改造）
 *
 * 流程：共享目录列表 → App 内目录树遍历（SAF，免系统选择器）→ 选中文件
 * 直接入队（local_path = content:// URI，M3 上传 SAF 流直传：宿主经
 * Kotlin safOpen/safRead 桥流式读，消除 v1 中转复制的双倍 IO）。
 *
 * 续传语义（spec M3）：可 seek（文件流）→ open 时 seek 到断点真续传；
 * pipe 流（不可 seek）→ 任务内断线重连保 fd 顺序续读、跨任务由宿主回报
 * not-seekable-resume 触发全量重传——均无需前端参与。
 *
 * 免授权特殊条目（app 私有下载目录）为真实路径：直读直传，与 SAF 条目
 * 同路径入队（引擎按 local_path 前缀分流）。
 */
import { ref, computed } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { SharedRoot, SharedEntry } from '../types'
import { KIND_PRIVATE_DOWNLOADS } from '../types'
import type { useTasks } from './useTasks'
import type { useSettings } from './useSettings'

type TasksApi = ReturnType<typeof useTasks>
type SettingsApi = ReturnType<typeof useSettings>

/** 目录树导航路径段 */
export interface CrumbsSegment {
  name: string
  documentId: string
}

export function useSharedUpload(
  context: PluginContext,
  tasks: TasksApi,
  settings: SettingsApi,
) {
  const open = ref(false)

  /** 当前目录树（含免授权特殊条目） */
  const currentRoot = ref<SharedRoot | null>(null)
  const entries = ref<SharedEntry[]>([])
  /** 面包屑路径（不含根） */
  const crumbs = ref<CrumbsSegment[]>([])
  const loading = ref(false)
  const listError = ref<string | null>(null)
  /** 失效标记（列表加载失败且为授权问题 → 提示重新授权） */
  const rootInvalid = ref(false)

  // ==================== 打开 / 浏览 ====================

  /** 打开上传页（打开前刷新设置与授权有效性） */
  async function openSheet(): Promise<void> {
    open.value = true
    // 打开时刷新共享目录列表（设置页可能刚添加过目录）
    await settings.load()
    if (currentRoot.value) {
      // 上次浏览的根仍存在 → 回到其根目录；否则回到根列表
      const stillExists = settings.settings.value.roots.some(
        (r) => r.id === currentRoot.value!.id,
      )
      if (stillExists) {
        const fresh = settings.settings.value.roots.find((r) => r.id === currentRoot.value!.id)!
        await enterRoot(fresh)
      } else {
        reset()
      }
    }
  }

  /** 进入某个共享目录根（面包屑回到根） */
  async function enterRoot(root: SharedRoot): Promise<void> {
    currentRoot.value = root
    crumbs.value = []
    rootInvalid.value = false
    await list(root, root.documentId)
  }

  /** 列出目录条目（SAF 树经新能力遍历；特殊条目为真实路径列表） */
  async function list(root: SharedRoot, documentId: string): Promise<void> {
    loading.value = true
    listError.value = null
    try {
      if (root.kind === KIND_PRIVATE_DOWNLOADS) {
        // 真实路径根（免授权特殊条目）无 SAF documentId 语义：子目录以
        // 面包屑名拼相对路径（listDir 白名单 canonicalize 前缀放行子路径）；
        // 忽略 documentId 参数，否则点击子目录永远重新列出根（同名目录
        // 无限点击、路径卡在一层）
        const rel = crumbs.value.map((c) => c.name).join('/')
        entries.value = await context.fileService.listDir(
          rel ? `${root.id}/${rel}` : root.id,
        )
      } else {
        entries.value = await context.fileService.saf.listTree(root.id, documentId)
      }
    } catch (e) {
      console.error(`[File Transfer] shared dir list failed: root=${root.id}`, e)
      listError.value = context.i18n.t('transfer.upload.dirUnavailable')
      // 授权被回收/目录被删（story #10）：标记失效并回写设置，展示重新授权入口
      if (root.kind !== KIND_PRIVATE_DOWNLOADS) {
        rootInvalid.value = true
        void settings.markRootInvalid(root.id)
      }
      entries.value = []
    } finally {
      loading.value = false
    }
  }

  /** 进入子目录 */
  async function cd(entry: SharedEntry): Promise<void> {
    if (!currentRoot.value) return
    crumbs.value = [...crumbs.value, { name: entry.name, documentId: entry.documentId }]
    await list(currentRoot.value, entry.documentId)
  }

  /** 返回上一级目录 */
  async function up(): Promise<void> {
    if (!currentRoot.value) return
    if (crumbs.value.length === 0) {
      currentRoot.value = null
      entries.value = []
      return
    }
    const prev = crumbs.value[crumbs.value.length - 2]
    crumbs.value = crumbs.value.slice(0, -1)
    await list(currentRoot.value, prev ? prev.documentId : currentRoot.value.documentId)
  }

  /** 回到共享目录根列表 */
  function goRoots(): void {
    reset()
  }

  /** 重置浏览状态，返回根列表视图 */
  function reset(): void {
    currentRoot.value = null
    entries.value = []
    crumbs.value = []
    loading.value = false
    listError.value = null
    rootInvalid.value = false
  }

  // ==================== 上传（M3 流直传直接入队） ====================

  /** 点击文件：直接入队（SAF 条目 local_path = content:// URI 流直传；特殊条目真实路径直传） */
  async function uploadFile(entry: SharedEntry): Promise<void> {
    if (!currentRoot.value) return
    if (!tasks.peerOnline.value) {
      context.dialogs.showToast(context.i18n.t('transfer.upload.offline'), 'error')
      return
    }
    const ok = await tasks.enqueueUpload({
      peerId: tasks.peerId.value,
      peerName: tasks.displayPeerName.value,
      remotePath: entry.name,
      localPath: entry.uri,
      // v2：声明文件大小（SAF 条目元信息；批请求 totalSize 与进度展示用）
      size: entry.size,
    })
    if (ok) {
      context.dialogs.showToast(context.i18n.t('transfer.upload.enqueued'), 'success')
    } else {
      // 入队失败（被拒/异常）：提示并复位（被拒场景 useTasks 已弹同名
      // 对话框，此 toast 兜底异常路径）
      context.dialogs.showToast(context.i18n.t('transfer.upload.enqueueFailed'), 'error')
    }
  }

  /** 关闭上传页（无进行中复制需要清理，仅重置视图） */
  function close(): void {
    open.value = false
    reset()
  }

  return {
    open,
    settings,
    currentRoot,
    entries,
    crumbs,
    loading,
    listError,
    rootInvalid,
    openSheet,
    enterRoot,
    cd,
    up,
    goRoots,
    uploadFile,
    close,
  }
}
