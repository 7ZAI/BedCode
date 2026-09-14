/**
 * Plugin Dialog Host 测试
 *
 * 验证队列 + Promise 关联逻辑（不依赖 Vue 组件渲染）
 */
import { describe, it, expect } from 'vitest'
import { pluginDialogHost } from '@/plugin/dialog-host'

describe('pluginDialogHost', () => {
  it('showConfirm 确认时返回 true，取消返回 false', async () => {
    const p = pluginDialogHost.showConfirm({ title: '确认删除？' })
    pluginDialogHost.resolveTop('confirm')
    await expect(p).resolves.toBe(true)

    const p2 = pluginDialogHost.showConfirm({ title: '确认删除？' })
    pluginDialogHost.resolveTop('cancel')
    await expect(p2).resolves.toBe(false)
  })

  it('showPrompt 确认时返回输入值，取消返回 null', async () => {
    const p = pluginDialogHost.showPrompt({ title: '输入名称' })
    pluginDialogHost.resolveTop('confirm', 'my-plugin')
    await expect(p).resolves.toBe('my-plugin')

    const p2 = pluginDialogHost.showPrompt({ title: '输入名称' })
    pluginDialogHost.resolveTop('cancel')
    await expect(p2).resolves.toBeNull()
  })

  it('showDialog 返回完整 DialogResult', async () => {
    const p = pluginDialogHost.showDialog({ title: '提示', message: '内容' })
    pluginDialogHost.resolveTop('cancel')
    await expect(p).resolves.toEqual({ action: 'cancel' })
  })

  it('多弹窗按 FIFO 顺序解析', async () => {
    const p1 = pluginDialogHost.showConfirm({ title: 'A' })
    const p2 = pluginDialogHost.showConfirm({ title: 'B' })
    pluginDialogHost.resolveTop('confirm')
    await expect(p1).resolves.toBe(true)
    pluginDialogHost.resolveTop('cancel')
    await expect(p2).resolves.toBe(false)
  })

  it('showPrompt 确认但无 value → 返回空串（r.value ?? 兜底）', async () => {
    const p = pluginDialogHost.showPrompt({ title: '输入名称' })
    pluginDialogHost.resolveTop('confirm')
    await expect(p).resolves.toBe('')
  })

  it('resolveById 定点结算队中条目（非队首，30s 超时关闭场景）', async () => {
    const p1 = pluginDialogHost.showConfirm({ title: 'A' })
    const p2 = pluginDialogHost.showConfirm({ title: 'B' })
    const p3 = pluginDialogHost.showConfirm({ title: 'C' })

    // 队中 id（p2）定点结算，不扰动队首 p1 与队尾 p3
    const targetId = pluginDialogHost.queue.value[1].id
    pluginDialogHost.resolveById(targetId, 'cancel')
    await expect(p2).resolves.toBe(false)

    // 剩余条目仍按 FIFO 顺序解析
    expect(pluginDialogHost.queue.value.map((i) => i.kind)).toEqual(['confirm', 'confirm'])
    pluginDialogHost.resolveTop('confirm')
    await expect(p1).resolves.toBe(true)
    pluginDialogHost.resolveTop('confirm')
    await expect(p3).resolves.toBe(true)
    expect(pluginDialogHost.queue.value).toHaveLength(0)
  })

  it('resolveById 未知 id → 不结算任何条目（队列保持原状）', async () => {
    const p = pluginDialogHost.showConfirm({ title: 'A' })
    pluginDialogHost.resolveById(999999, 'cancel')
    // promise 仍未结算
    let settled = false
    void p.then(() => {
      settled = true
    })
    await Promise.resolve()
    await Promise.resolve()
    expect(settled).toBe(false)
    // 队列未被移除
    expect(pluginDialogHost.queue.value).toHaveLength(1)
    // 队首仍可正常结算
    pluginDialogHost.resolveTop('cancel')
    await expect(p).resolves.toBe(false)
  })

  it('resolveById 支持携带 value（showPrompt 场景）', async () => {
    const p = pluginDialogHost.showPrompt({ title: '输入名称' })
    const targetId = pluginDialogHost.queue.value[0].id
    pluginDialogHost.resolveById(targetId, 'confirm', 'from-id')
    await expect(p).resolves.toBe('from-id')
  })
})
