/**
 * 批量传输请求全局弹窗（移动端宿主全局弹窗版）
 *
 * 与桌面端 FileTransferView + useBatchPrompt + context.ui.showDialog 同构，
 * 但生命周期为插件激活期常驻（index.ts activate 时 start、deactivate 时 stop），
 * 不依赖 FileTransferView 挂载——即使用户处于别的页面（工具箱其他页、设置等），
 * 宿主全局弹窗仍能覆盖全屏弹出（Teleport to body，z-100）。
 *
 * 语义与桌面端一致：
 * - 多个 pending 批按创建时间升序逐个提示（先到先弹）
 * - 已提示过的批（超时/应答后宿主移除前）不再重复弹出
 * - 当前批从列表消失（用户应答 / 宿主 TTL 超时）→ 推进下一个未提示批
 * - 超时默认拒绝由宿主 TTL 执行（reason=timeout），前端仅标记已提示并推进
 *
 * 弹窗由宿主 PluginGlobalDialog 渲染（预设模式：title/message/actions/countdownLabel/deadlineAt），
 * 队列由 SDK global-dialog 控制器 FIFO 管理。
 */

import { ref, type Ref } from 'vue'
import type { Disposable, PluginContext, PluginDialogHandle } from '@binblink/bedcode-plugin-sdk-mobile'
import type { PendingBatch } from '../types'
import { formatBytes } from '../utils/format'

// ==================== 纯函数：wire → 内部模型 ====================

function mapWireBatch(raw: any): PendingBatch {
  const files = Array.isArray(raw.files) ? raw.files : []
  return {
    batchId: raw.batchId ?? '',
    peerName: raw.peerName ?? '',
    files: files.map((f: any) => ({
      relativePath: f.path ?? f.relativePath ?? '',
      size: f.size ?? 0,
    })),
    totalSize: raw.totalBytes ?? 0,
    createdAt: raw.createdAtMs ?? 0,
  }
}

// ==================== 模块级单例状态 ====================

const current: Ref<PendingBatch | null> = ref(null)
const pendingCount: Ref<number> = ref(0)

let batches: PendingBatch[] = []
const prompted = new Set<string>()

let boundContext: PluginContext | null = null
let subscription: Disposable | null = null
let started = false

let dialogHandle: PluginDialogHandle | null = null
let approvalTimeoutSec = 60

function syncPendingCount(): void {
  pendingCount.value = batches.length
}

function nextUnprompted(): PendingBatch | null {
  const candidates = batches
    .filter((b) => !prompted.has(b.batchId))
    .sort((a, b) => a.createdAt - b.createdAt)
  return candidates[0] ?? null
}

function markAnswered(batchId: string): void {
  prompted.add(batchId)
}

function markTimeout(): void {
  const cur = current.value
  if (!cur) return
  prompted.add(cur.batchId)
  advance()
}

function buildDialogOptions(batch: PendingBatch, ctx: PluginContext): import('@binblink/bedcode-plugin-sdk-mobile').PluginDialogOptions {
  const t = (key: string, params?: Record<string, unknown>) => ctx.i18n.t(key, params)
  // 文件传输业务恒需要倒计时（验收：接收端倒计时提示不可缺失）。
  // createdAt 缺失/异常（wire 缺字段）时以当前时间兜底计算 deadline，
  // 保证 countdownLabel + deadlineAt 恒在；宿主组件仅在两者齐备时渲染倒计时
  const createdAtMs = batch.createdAt > 0 ? batch.createdAt : Date.now()
  const deadlineAt = createdAtMs + approvalTimeoutSec * 1000
  return {
    title: t('transfer.request.title'),
    message: t('transfer.request.body', {
      name: batch.peerName || t('transfer.peer.unknown'),
      count: batch.files?.length ?? 1,
      size: formatBytes(batch.totalSize, t),
    }),
    countdownLabel: t('transfer.request.countdown'),
    deadlineAt,
    actions: [
      {
        label: t('transfer.request.rejectAll'),
        kind: 'default',
        onClick: async () => {
          markAnswered(batch.batchId)
          try {
            await ctx.commands.execute('file-transfer.reject-batch', { batchId: batch.batchId })
          } catch (e) {
            console.error(`[File Transfer] reject-batch failed for "${batch.batchId}":`, e)
          }
          // 应答后批仍在列表中直到下一次 batches-changed 推送移除；
          // 提前推进避免用户需等待推送才看到下一批（宿主推送通常 < 200ms）
          advance()
        },
      },
      {
        label: t('transfer.request.acceptAll'),
        kind: 'primary',
        onClick: async () => {
          markAnswered(batch.batchId)
          try {
            await ctx.commands.execute('file-transfer.approve-batch', { batchId: batch.batchId })
          } catch (e) {
            console.error(`[File Transfer] approve-batch failed for "${batch.batchId}":`, e)
          }
          advance()
        },
      },
    ],
    closable: false,
    closeOnBackdrop: false,
    onTimeout: () => {
      markTimeout()
    },
    onClose: () => {
      dialogHandle = null
      // 关闭后若仍有待提示批（超时或应答后未被批次推送覆盖的场景），尝试续弹
      if (current.value == null) {
        const next = nextUnprompted()
        if (next) {
          current.value = next
          openDialog(next)
        }
      }
    },
  }
}

function openDialog(batch: PendingBatch): void {
  if (!boundContext) return
  const opts = buildDialogOptions(batch, boundContext)
  if (!dialogHandle) {
    dialogHandle = boundContext.ui.showDialog(opts)
  } else {
    dialogHandle.update(opts)
  }
}

function closeDialog(): void {
  if (dialogHandle) {
    const h = dialogHandle
    dialogHandle = null
    h.close()
  }
}

function advance(): void {
  // 当前批已结算，推进到下一个未提示批
  const next = nextUnprompted()
  if (next) {
    current.value = next
    openDialog(next)
  } else {
    current.value = null
    closeDialog()
  }
}

function sync(): void {
  const cur = current.value
  // 当前批已被移除（应答 / 超时 TTL 回收）→ 关闭并推进
  if (cur && !batches.some((b) => b.batchId === cur.batchId)) {
    // 若宿主已通过TTL移除， prompted 中可能尚无记录，补标记防止重弹
    prompted.add(cur.batchId)
    const next = nextUnprompted()
    if (next) {
      current.value = next
      openDialog(next)
    } else {
      current.value = null
      closeDialog()
    }
    return
  }
  if (!cur) {
    const next = nextUnprompted()
    if (next) {
      current.value = next
      openDialog(next)
    }
  } else {
    // 列表刷新但当前批仍存在 → 热更新倒计时/文案（超时配置可能已变更）
    openDialog(cur)
  }
}

async function loadInitial(): Promise<void> {
  if (!boundContext) return
  try {
    const [rawBatches, rawSettings] = await Promise.all([
      boundContext.commands.execute('file-transfer.list-batches', {}).catch(() => []),
      boundContext.commands.execute('file-transfer.get-settings', {}).catch(() => null),
    ])
    if (Array.isArray(rawBatches)) {
      batches = rawBatches.map(mapWireBatch)
      syncPendingCount()
    }
    if (rawSettings && typeof rawSettings === 'object') {
      const sec = (rawSettings as any).ask_timeout_sec ?? (rawSettings as any).askTimeoutSec
      if (typeof sec === 'number' && sec > 0) approvalTimeoutSec = sec
    }
    // 初始批到达后同步弹窗
    sync()
  } catch (e) {
    console.error('[File Transfer] batch dialog initial load failed:', e)
  }
}

function handleBatchesChanged(payload: unknown): void {
  if (!Array.isArray(payload)) return
  batches = payload.map(mapWireBatch)
  syncPendingCount()
  sync()
}

// ==================== 控制器 ====================

export interface BatchDialogController {
  current: Ref<PendingBatch | null>
  pendingCount: Ref<number>
  /** 幂等启动：激活期常驻订阅（index.ts activate 调用；重复调用不叠加） */
  start(): void
  /** 停止订阅并关闭弹窗（deactivate 对称清理） */
  stop(): void
  /** 标记当前批超时并推进（供测试/宿主 onTimeout 回调） */
  markTimeout(): void
}

export function useBatchDialog(context: PluginContext): BatchDialogController {
  return {
    current,
    pendingCount,
    start() {
      boundContext = context
      if (started) return
      started = true
      // 监听设置变更：若宿主后续提供 settings-changed 事件，可在此订阅并更新 approvalTimeoutSec
      subscription = context.events.on('plugin:file-transfer:batches-changed', handleBatchesChanged)
      void loadInitial()
    },
    stop() {
      subscription?.dispose()
      subscription = null
      started = false
      batches = []
      prompted.clear()
      current.value = null
      pendingCount.value = 0
      approvalTimeoutSec = 60
      closeDialog()
      boundContext = null
    },
    markTimeout,
  }
}

/** 测试辅助：重置模块级状态 */
export function _resetBatchDialogForTest(): void {
  subscription?.dispose()
  subscription = null
  started = false
  batches = []
  prompted.clear()
  current.value = null
  pendingCount.value = 0
  approvalTimeoutSec = 60
  if (dialogHandle) {
    dialogHandle.close()
    dialogHandle = null
  }
  boundContext = null
}
