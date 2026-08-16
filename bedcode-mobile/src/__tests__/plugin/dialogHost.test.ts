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
})
