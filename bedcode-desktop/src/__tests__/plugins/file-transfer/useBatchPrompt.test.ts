/**
 * useBatchPrompt 编排测试（接收端待确认批的选择/推进）
 *
 * 宿主全局弹窗迁移后，批次弹窗的「当前提示哪一批」状态机集中在本 composable：
 * 多个 pending 批先到先弹、已提示批不再重复、当前批 resolved 自动推进、
 * 超时（markTimeout）标记后由宿主 TTL 拒绝。只测状态机，宿主弹窗渲染由
 * packages/plugin-sdk-desktop/__tests__/global-dialog.test.ts 覆盖。
 */
import { describe, it, expect } from 'vitest'
import { nextTick, ref } from 'vue'
import type { PendingBatch } from '../../../../plugins/file-transfer/src/types'
import { useBatchPrompt } from '../../../../plugins/file-transfer/src/composables/useBatchPrompt'

function makeBatch(id: string, createdAt: number): PendingBatch {
  return {
    batchId: id,
    peerId: 'node-a',
    peerName: '设备-a',
    files: [{ relativePath: 'a.txt', size: 10 }],
    totalSize: 10,
    createdAt,
  }
}

describe('useBatchPrompt', () => {
  it('有待确认批时按创建时间升序选择当前批', () => {
    const batches = ref<PendingBatch[]>([makeBatch('b-2', 200), makeBatch('b-1', 100)])
    const prompt = useBatchPrompt(batches)
    expect(prompt.current.value).toMatchObject({ batchId: 'b-1' })
  })

  it('当前批从列表消失（已应答）→ 自动推进到下一个未提示批', async () => {
    const batches = ref<PendingBatch[]>([makeBatch('b-1', 100), makeBatch('b-2', 200)])
    const prompt = useBatchPrompt(batches)

    // 宿主 resolve 移除 b-1 → 推进 b-2（watch 异步 flush，需 nextTick）
    batches.value = [makeBatch('b-2', 200)]
    await nextTick()
    expect(prompt.current.value).toMatchObject({ batchId: 'b-2' })

    // 全部清空 → 无当前批
    batches.value = []
    await nextTick()
    expect(prompt.current.value).toBeNull()
  })

  it('markTimeout：标记已提示并推进（拒绝由宿主 TTL 执行，前端不主动拒绝）', async () => {
    const batches = ref<PendingBatch[]>([makeBatch('b-1', 100), makeBatch('b-2', 200)])
    const prompt = useBatchPrompt(batches)
    expect(prompt.current.value).toMatchObject({ batchId: 'b-1' })

    prompt.markTimeout()
    expect(prompt.current.value).toMatchObject({ batchId: 'b-2' })

    // 超时批仍留在列表（TTL 未清扫）时不再重复提示
    prompt.markTimeout()
    expect(prompt.current.value).toBeNull()
    batches.value = [makeBatch('b-1', 100), makeBatch('b-2', 200)]
    await nextTick()
    expect(prompt.current.value).toBeNull()
  })

  it('markAnswered：应答后立即标记，防止同批在 host 移除前重复弹出', () => {
    const batches = ref<PendingBatch[]>([makeBatch('b-1', 100)])
    const prompt = useBatchPrompt(batches)
    prompt.markAnswered()
    expect(prompt.current.value).toBeNull()
    // host 尚未移除该批（临时列表快照）→ 不重新弹出
    batches.value = [makeBatch('b-1', 100)]
    expect(prompt.current.value).toBeNull()
  })

  it('批列表整体重建（同批 ID）时当前批不漂移（保持原 deadline 语义）', async () => {
    const batches = ref<PendingBatch[]>([makeBatch('b-1', 100)])
    const prompt = useBatchPrompt(batches)
    const first = prompt.current.value
    // 列表重建（事件整表替换），同 batchId
    batches.value = [makeBatch('b-1', 100)]
    await nextTick()
    expect(prompt.current.value).toBe(first)
  })
})