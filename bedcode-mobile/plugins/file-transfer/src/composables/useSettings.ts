/**
 * 插件设置核心逻辑 (Mobile)
 *
 * 经 `file-transfer.get-settings` / `set-settings` 读写，WASM 侧持久化到 storage。
 * 共享目录条目为 SAF URI 存储（content://tree/... + 持久化授权），经系统
 * 目录树选择器添加（fileService.pickSharedDirectory）；免授权特殊条目
 * （app 私有下载目录）由 WASM 侧派生注入（kind=private_downloads，不可移除）。
 * 下载目录为只读展示（下载固定落系统 AppDownloadsDir）。
 */
import { ref } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { Settings, SharedRoot } from '../types'
import { KIND_PRIVATE_DOWNLOADS } from '../types'
import { MOCK_ENABLED } from '../mock'

/** 并发数上限（与 WASM Queue 一致） */
export const CONCURRENCY_MAX = 8

/**
 * 从 SAF 树 document id 派生展示名（displayName 缺失时兜底）
 *
 * documentId 为 Kotlin 解码形态（primary:下载文件夹 / 0123-4567:DCIM/相机），
 * 取末段并剥 volume 前缀；空则回退完整 URI（最坏情况仍是可辨识的原始值）
 */
function deriveName(documentId: string): string {
  const last = documentId.split('/').filter(Boolean).pop() ?? ''
  const name = last.includes(':') ? (last.split(':').pop() ?? '') : last
  return name || ''
}

/** 将 WASM get-settings 返回（snake_case download_dir / document_id）归一化为 camelCase */
function mapWireRoot(raw: any): SharedRoot {
  return {
    id: raw?.id ?? '',
    kind: raw?.kind === KIND_PRIVATE_DOWNLOADS ? KIND_PRIVATE_DOWNLOADS : 'saf',
    name: raw?.name ?? '',
    documentId: raw?.document_id ?? raw?.documentId ?? '',
    authorized: raw?.authorized !== false,
  }
}

/** 将 WASM get-settings 返回（snake_case download_dir）归一化为 camelCase */
function mapWireSettings(raw: any): Settings {
  const policy = raw?.receiving_policy ?? raw?.receivingPolicy ?? 'ask'
  return {
    roots: Array.isArray(raw?.roots) ? raw.roots.map(mapWireRoot) : [],
    downloadDir: raw?.download_dir ?? raw?.downloadDir ?? '',
    concurrency: raw?.concurrency ?? 3,
    receivingPolicy: policy === 'accept' || policy === 'reject' ? policy : 'ask',
    approvalTimeoutSec: raw?.approval_timeout_sec ?? raw?.approvalTimeoutSec ?? 60,
  }
}

export function useSettings(context: PluginContext) {
  const settings = ref<Settings>({
    roots: [],
    downloadDir: '',
    concurrency: 3,
    receivingPolicy: 'ask',
    approvalTimeoutSec: 60,
  })
  const loading = ref(false)

  /** 加载设置（含首次拉取；mock 下返回演示配置） */
  async function load(): Promise<void> {
    loading.value = true
    try {
      if (MOCK_ENABLED) {
        settings.value = {
          roots: [
            { id: 'content://tree/mock-shared', kind: 'saf', name: '模拟共享目录', documentId: 'mock:root', authorized: true },
            { id: '/mock/downloads', kind: KIND_PRIVATE_DOWNLOADS, name: '下载目录', documentId: '', authorized: true },
          ],
          downloadDir: '/storage/emulated/0/Download',
          concurrency: 4,
          receivingPolicy: 'ask',
          approvalTimeoutSec: 60,
        }
        return
      }
      const data = await context.commands.execute('file-transfer.get-settings', {})
      settings.value = mapWireSettings(data)
      // 共享目录授权有效性刷新：授权被回收/目录被删 → 标记失效（story #10）
      await refreshRootsAuth()
      // 中转复制残留清扫（spec「复制桥语义」：激活时扫描清理 cache 残留；
      // 前端 activate 可能早于宿主置 Activated 导致门控拒绝，此处兜底幂等重试）
      void context.fileService.saf.cleanupStaleCopies().catch(() => {})
    } catch (e) {
      console.error('[File Transfer] get-settings failed:', e)
    } finally {
      loading.value = false
    }
  }

  /** 逐条检测 SAF 条目授权有效性，失效标记回写设置（特殊条目恒有效） */
  async function refreshRootsAuth(): Promise<void> {
    const checked = await Promise.all(
      settings.value.roots.map(async (root) => {
        if (root.kind === KIND_PRIVATE_DOWNLOADS) return root
        try {
          const authorized = await context.fileService.saf.checkAuthorized(root.id)
          return authorized === root.authorized ? root : { ...root, authorized }
        } catch {
          return root
        }
      }),
    )
    if (checked.some((r, i) => r.authorized !== settings.value.roots[i].authorized)) {
      settings.value = { ...settings.value, roots: checked }
    }
  }

  /**
   * 追加共享目录（经系统目录树选择器；SAF URI 条目）
   *
   * 返回结果原因供 UI 精确提示：ok（已保存并挂载）/ duplicate（已存在）/
   * cancelled（用户取消，静默）/ failed（保存失败）/ unsupported（平台不支持 SAF）。
   */
  async function addRoot(): Promise<'ok' | 'duplicate' | 'cancelled' | 'failed' | 'unsupported'> {
    let picked: { uri: string; documentId: string; displayName: string } | null
    try {
      picked = await context.fileService.pickSharedDirectory()
    } catch {
      return 'unsupported'
    }
    if (!picked) return 'cancelled' // 用户取消
    if (settings.value.roots.some((r) => r.id === picked.uri)) return 'duplicate'
    const entry: SharedRoot = {
      id: picked.uri,
      kind: 'saf',
      name: picked.displayName || deriveName(picked.documentId) || picked.uri,
      documentId: picked.documentId,
      authorized: true,
    }
    const next = [...settings.value.roots, entry]
    return (await persist({ roots: next })) ? 'ok' : 'failed'
  }

  /** 重新授权失效条目：重新选择目录树并替换原条目 */
  async function reauthorizeRoot(root: SharedRoot): Promise<boolean> {
    let picked: { uri: string; documentId: string; displayName: string } | null
    try {
      picked = await context.fileService.pickSharedDirectory()
    } catch {
      return false
    }
    if (!picked) return false
    const next = settings.value.roots.map((r) =>
      r.id === root.id
        ? { ...r, id: picked.uri, name: picked.displayName || deriveName(picked.documentId) || picked.uri, documentId: picked.documentId, authorized: true }
        : r,
    )
    return persist({ roots: next })
  }

  /** 移除共享目录（免授权特殊条目不可移除） */
  async function removeRoot(id: string): Promise<boolean> {
    const next = settings.value.roots.filter((r) => r.id !== id && r.kind !== KIND_PRIVATE_DOWNLOADS)
    return persist({ roots: next })
  }

  /** 标记条目失效（列表加载失败且为授权问题时的回写；特殊条目不可标记） */
  async function markRootInvalid(id: string): Promise<void> {
    const root = settings.value.roots.find((r) => r.id === id)
    if (!root || root.kind === KIND_PRIVATE_DOWNLOADS || !root.authorized) return
    const next = settings.value.roots.map((r) => (r.id === id ? { ...r, authorized: false } : r))
    settings.value = { ...settings.value, roots: next }
    await persist({ roots: next })
  }

  /** 设置并发数（1–8） */
  async function setConcurrency(n: number): Promise<boolean> {
    const clamped = Math.min(Math.max(Math.round(n), 1), CONCURRENCY_MAX)
    return persist({ concurrency: clamped })
  }

  /** v2 设置接收策略（ask/accept/reject；本地生效，发送方不感知） */
  async function setReceivingPolicy(
    policy: 'ask' | 'accept' | 'reject',
  ): Promise<boolean> {
    return persist({ receivingPolicy: policy })
  }

  /** v2 设置同意超时（秒，10–600，仅 ask 策略生效；越界 clamp） */
  async function setApprovalTimeout(secs: number): Promise<boolean> {
    const clamped = Math.min(Math.max(Math.round(secs), 10), 600)
    return persist({ approvalTimeoutSec: clamped })
  }

  /** 写入 WASM 并同步本地状态（挂载失败时 set-settings 返回错误 → false；mock 下直接本地持久） */
  async function persist(patch: Partial<Settings>): Promise<boolean> {
    if (MOCK_ENABLED) {
      settings.value = { ...settings.value, ...patch }
      return true
    }
    try {
      await context.commands.execute('file-transfer.set-settings', {
        roots: (patch.roots ?? settings.value.roots).map((r) => ({
          id: r.id,
          kind: r.kind,
          name: r.name,
          document_id: r.documentId,
          authorized: r.authorized,
        })),
        // 不回传 downloadDir：下载目录为只读展示（M1 无选择入口），全量回传
        // 会把 WASM get-settings 派生的默认值写回 storage，静默覆写存储值
        concurrency: patch.concurrency ?? settings.value.concurrency,
        receivingPolicy: patch.receivingPolicy ?? settings.value.receivingPolicy,
        approvalTimeoutSec: patch.approvalTimeoutSec ?? settings.value.approvalTimeoutSec,
      })
      settings.value = { ...settings.value, ...patch }
      return true
    } catch (e) {
      console.error('[File Transfer] set-settings failed:', e)
      return false
    }
  }

  return {
    settings,
    loading,
    load,
    addRoot,
    reauthorizeRoot,
    removeRoot,
    markRootInvalid,
    setConcurrency,
    setReceivingPolicy,
    setApprovalTimeout,
  }
}
