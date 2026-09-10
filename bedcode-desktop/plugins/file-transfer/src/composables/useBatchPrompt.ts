/**
 * 待确认批提示选择器（接收端应答，spec 14.4）
 *
 * 由宿主全局弹窗渲染（FileTransferView 经 context.ui.showDialog 预设模式），
 * 本 composable 只负责「当前应提示哪一批」的状态机：
 * - 多个 pending 批按创建时间升序逐个提示（先到先弹）；
 * - 已提示过的批（超时/应答后 host 移除前）不再重复弹出；
 * - 当前批从列表中消失（用户应答 / 宿主 TTL 结算）→ 推进到下一个未提示批；
 * - 超时默认拒绝由宿主 TTL 执行（reason=timeout），前端不主动 reject，
 *   仅把该批标记为已提示并推进。
 *
 * 视图作用域（随 FileTransferView 生命周期）：宿主全局弹窗在面板 KeepAlive
 * 切走后仍可见可操作，不受本状态机所在视图挂载位置影响。
 */
import { ref, watch, type Ref } from 'vue'
import type { PendingBatch } from '../types'

export function useBatchPrompt(batches: Ref<PendingBatch[]>) {
  /** 当前弹窗展示的批（null = 无待提示批） */
  const current = ref<PendingBatch | null>(null)
  /** 已提示过的批 ID（防超时后同批重复弹出；应答后批被 host 移除不再回见） */
  const prompted = new Set<string>()

  /** 取第一个未提示的 pending 批（按创建时间升序 = 先到先弹） */
  function nextUnprompted(list: PendingBatch[]): PendingBatch | null {
    return (
      list
        .filter((b) => !prompted.has(b.batchId))
        .sort((a, b) => a.createdAt - b.createdAt)[0] ?? null
    )
  }

  /** 批列表变化：当前批被 resolved → 关闭并提示下一批；无当前批 → 弹第一个未提示批 */
  function sync(): void {
    const cur = current.value
    if (cur && !batches.value.some((b) => b.batchId === cur.batchId)) {
      current.value = nextUnprompted(batches.value)
      return
    }
    if (!cur) current.value = nextUnprompted(batches.value)
  }

  // batche-s 事件每次由父组件整体重建（引用变化），deep 监听列表内容以捕获批内字段更新
  watch(() => batches.value, sync, { deep: true, immediate: true })

  /** 超时（宿主弹窗 onTimeout）：标记已提示并推进下一批（拒绝由宿主 TTL 执行） */
  function markTimeout(): void {
    const cur = current.value
    if (!cur) return
    prompted.add(cur.batchId)
    current.value = nextUnprompted(batches.value)
  }

  /** 应答后由调用方标记（批被 host 移除前防止重复提示） */
  function markAnswered(): void {
    const cur = current.value
    if (!cur) return
    prompted.add(cur.batchId)
    current.value = nextUnprompted(batches.value)
  }

  return {
    current,
    markTimeout,
    markAnswered,
  }
}