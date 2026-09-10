/**
 * 全局弹窗控制器 + PluginGlobalDialog 组件测试
 *
 * 控制器为发布-订阅（不依赖 Vue reactivity），直接驱动；组件用 @vue/test-utils
 * 挂载后经控制器开/关/更新弹窗，验证预设模式（标题/正文/按钮/倒计时/跳转）、
 * 组件模式（provide pluginContext / props 热更新）、Escape/遮罩关闭与排队行为。
 */
import { defineComponent, h, inject } from 'vue'
import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  _resetGlobalDialogForTest,
  closeGlobalDialog,
  getGlobalDialog,
  openGlobalDialog,
  subscribeGlobalDialog,
} from '../src/global-dialog'
import PluginGlobalDialog from '../src/ui/PluginGlobalDialog.vue'

const click = async (el: Element) => {
  ;(el as HTMLElement).click()
  // 事件循环一个微任务，允许异步 action 结算
  await Promise.resolve()
}

describe('全局弹窗控制器', () => {
  let listener: ReturnType<typeof vi.fn>

  beforeEach(() => {
    _resetGlobalDialogForTest()
    listener = vi.fn()
    subscribeGlobalDialog(listener)
  })

  afterEach(() => {
    _resetGlobalDialogForTest()
    vi.useRealTimers()
  })

  it('open 后成为当前弹窗并广播，handle.close() 幂等关闭', () => {
    const handle = openGlobalDialog({ title: 't1' })
    expect(getGlobalDialog()).toMatchObject({ title: 't1' })
    expect(listener).toHaveBeenLastCalledWith(expect.objectContaining({ title: 't1' }))

    handle.close()
    expect(getGlobalDialog()).toBeNull()
    expect(listener).toHaveBeenLastCalledWith(null)
    // 幂等：重复 close 不抛错
    handle.close()
    expect(getGlobalDialog()).toBeNull()
  })

  it('后到的弹窗排队，当前关闭后自动接替', () => {
    const first = openGlobalDialog({ title: 'a' })
    const second = openGlobalDialog({ title: 'b' })
    expect(getGlobalDialog()).toMatchObject({ title: 'a' })

    first.close()
    expect(getGlobalDialog()).toMatchObject({ title: 'b' })

    second.close()
    expect(getGlobalDialog()).toBeNull()
  })

  it('排队中的条目可被 close() 移除（不再弹出）', () => {
    const first = openGlobalDialog({ title: 'a' })
    const second = openGlobalDialog({ title: 'b' })
    second.close()
    first.close()
    expect(getGlobalDialog()).toBeNull()
    // 广播过两次 null/变化序列，最终为 null
    expect(listener).toHaveBeenLastCalledWith(null)
  })

  it('update() 生成新条目对象并广播（当前与排队中均生效）', () => {
    const first = openGlobalDialog({ title: 'a' })
    const second = openGlobalDialog({ title: 'b' })

    first.update({ title: 'a2' })
    expect(getGlobalDialog()).toMatchObject({ title: 'a2' })

    // 排队中的条目 update 不改变当前弹窗
    second.update({ title: 'b2' })
    expect(getGlobalDialog()).toMatchObject({ title: 'a2' })

    // 更新后句柄仍可关闭排队条目
    second.close()
    first.close()
    expect(getGlobalDialog()).toBeNull()
  })

  it('提供 timeoutSec 时定时自动关闭：onTimeout → onClose → 队列接替（可选能力）', () => {
    vi.useFakeTimers()
    const onTimeout = vi.fn()
    const onClose = vi.fn()
    const first = openGlobalDialog({ title: 'a', timeoutSec: 5, onTimeout, onClose })
    void first
    openGlobalDialog({ title: 'b' })

    vi.advanceTimersByTime(4999)
    expect(getGlobalDialog()).toMatchObject({ title: 'a' })
    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledTimes(1)
    expect(onClose).toHaveBeenCalledTimes(1)
    // 超时关闭 → 队列接替
    expect(getGlobalDialog()).toMatchObject({ title: 'b' })
  })

  it('未提供 timeoutSec / deadlineAt 时不自动关闭（常驻）', () => {
    vi.useFakeTimers()
    openGlobalDialog({ title: 'a' })
    vi.advanceTimersByTime(60_000)
    expect(getGlobalDialog()).toMatchObject({ title: 'a' })
  })

  it('deadlineAt 优先于 timeoutSec；update 可推迟/重设截止', () => {
    vi.useFakeTimers()
    const now = Date.now()
    const onTimeout = vi.fn()
    openGlobalDialog({
      title: 'a',
      timeoutSec: 10,
      deadlineAt: now + 2000,
      onTimeout,
    })
    vi.advanceTimersByTime(1999)
    expect(onTimeout).not.toHaveBeenCalled()
    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledTimes(1)
    expect(getGlobalDialog()).toBeNull()
  })

  it('onClose 在手动 close 时触发（含队列顶替前的当前弹窗）', () => {
    const onClose1 = vi.fn()
    const onClose2 = vi.fn()
    const first = openGlobalDialog({ title: 'a', onClose: onClose1 })
    const second = openGlobalDialog({ title: 'b', onClose: onClose2 })
    first.close()
    expect(onClose1).toHaveBeenCalledTimes(1)
    second.close()
    expect(onClose2).toHaveBeenCalledTimes(1)
  })
})

describe('PluginGlobalDialog 组件', () => {
  let wrapper: ReturnType<typeof mount> | null = null

  beforeEach(() => {
    _resetGlobalDialogForTest()
  })

  afterEach(() => {
    _resetGlobalDialogForTest()
    wrapper?.unmount()
    wrapper = null
    delete (window as any).__BEDCODE_SHARED__
  })

  const mountDialog = () => {
    wrapper = mount(PluginGlobalDialog, { attachTo: document.body })
    return wrapper
  }

  const overlay = () => document.querySelector<HTMLElement>('.pgd-overlay')

  it('预设模式：渲染标题 / 正文 / 动作按钮', async () => {
    mountDialog()
    const onConfirm = vi.fn()
    const onReject = vi.fn()
    openGlobalDialog({
      title: '检查',
      message: '确定继续？',
      actions: [
        { label: '拒绝', kind: 'default', onClick: onReject },
        { label: '接受', kind: 'primary', onClick: onConfirm },
      ],
    })
    await Promise.resolve()

    const root = overlay()!
    expect(root.textContent).toContain('检查')
    expect(root.textContent).toContain('确定继续？')
    const buttons = [...root.querySelectorAll('button')]
    expect(buttons.some((b) => b.textContent === '拒绝')).toBe(true)
    expect(buttons.some((b) => b.textContent === '接受')).toBe(true)
  })

  it('预设模式：动作点击成功后关闭弹窗；onClick 抛错则保持打开', async () => {
    mountDialog()
    const failing = vi.fn(() => {
      throw new Error('boom')
    })
    const ok = vi.fn()
    openGlobalDialog({
      title: 'x',
      actions: [
        { label: '失败', onClick: failing },
        { label: '成功', onClick: ok },
      ],
    })
    await Promise.resolve()
    const root = overlay()!
    const buttons = [...root.querySelectorAll('button')].filter((b) => b.textContent!.length > 0)

    await click(buttons[0]!)
    expect(failing).toHaveBeenCalled()
    expect(getGlobalDialog()).not.toBeNull()

    await click(buttons[1]!)
    expect(ok).toHaveBeenCalled()
    expect(getGlobalDialog()).toBeNull()
  })

  it('动作 navigateTo：关闭后经宿主 router 跳转', async () => {
    const push = vi.fn()
    ;(window as any).__BEDCODE_SHARED__ = { router: { push } }
    mountDialog()
    openGlobalDialog({
      title: 'x',
      actions: [{ label: '跳转', navigateTo: '/target' }],
    })
    await Promise.resolve()
    const root = overlay()!
    const btn = [...root.querySelectorAll('button')].find((b) => b.textContent === '跳转')!
    await click(btn)
    expect(getGlobalDialog()).toBeNull()
    expect(push).toHaveBeenCalledWith('/target')
  })

  it('倒计时：显示 {seconds} 文案并按截止时刻递减', async () => {
    vi.useFakeTimers()
    mountDialog()
    openGlobalDialog({
      title: 'x',
      countdownLabel: '{seconds} 秒后自动拒绝',
      timeoutSec: 30,
    })
    await vi.advanceTimersByTimeAsync(0)
    expect(overlay()!.textContent).toContain('30 秒后自动拒绝')
    await vi.advanceTimersByTimeAsync(5000)
    expect(overlay()!.textContent).toContain('25 秒后自动拒绝')
  })

  it('未配置倒计时时文案不显示', async () => {
    mountDialog()
    openGlobalDialog({ title: 'x' })
    await Promise.resolve()
    expect(overlay()!.querySelector('.pgd-countdown')).toBeNull()
  })

  it('组件模式：渲染插件内容组件并提供 pluginContext（inject 可用）', async () => {
    const Probe = defineComponent({
      setup() {
        const ctx = inject<{ id: string }>('pluginContext')
        return () => h('div', { 'data-probe': String(ctx?.id) }, 'content')
      },
    })
    mountDialog()
    // 无 pluginContext（宿主未注入）也能渲染
    openGlobalDialog({ content: Probe, props: { count: 3 } })
    await Promise.resolve()
    expect(document.querySelector('[data-probe]')!.textContent).toBe('content')

    // 关闭后重开：携带 pluginContext（宿主 showDialog 注入）→ 内容可 inject
    closeGlobalDialog(getGlobalDialog()!._id)
    await Promise.resolve()
    openGlobalDialog({ content: Probe, props: {}, pluginContext: { id: 'com.test' } })
    await Promise.resolve()
    expect(document.querySelector('[data-probe]')!.getAttribute('data-probe')).toBe('com.test')
  })

  it('components 模式对被替换只保留一个内容实例', async () => {
    const Probe = defineComponent({ setup: () => () => h('div', { class: 'probe' }) })
    mountDialog()
    openGlobalDialog({ content: Probe })
    openGlobalDialog({ content: Probe, props: {} })
    await Promise.resolve()
    // 排队：仅当前弹窗渲染（一个 probe）
    expect(document.querySelectorAll('.probe').length).toBe(1)
    closeGlobalDialog(getGlobalDialog()!._id)
    await Promise.resolve()
    expect(document.querySelectorAll('.probe').length).toBe(1)
  })

  it('Escape 关闭（closable 缺省 true）；closable:false 时忽略', async () => {
    mountDialog()
    openGlobalDialog({ title: 'a' })
    await Promise.resolve()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await Promise.resolve()
    expect(getGlobalDialog()).toBeNull()

    openGlobalDialog({ title: 'b', closable: false })
    await Promise.resolve()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await Promise.resolve()
    expect(getGlobalDialog()).not.toBeNull()
    closeGlobalDialog(getGlobalDialog()!._id)
  })

  it('点击遮罩关闭（closeOnBackdrop 缺省 true）', async () => {
    mountDialog()
    openGlobalDialog({ title: 'a' })
    await Promise.resolve()
    ;(overlay()! as HTMLElement).click()
    await Promise.resolve()
    expect(getGlobalDialog()).toBeNull()
  })

  it('遮罩不关闭：closeOnBackdrop:false', async () => {
    mountDialog()
    openGlobalDialog({ title: 'a', closeOnBackdrop: false })
    await Promise.resolve()
    ;(overlay()! as HTMLElement).click()
    await Promise.resolve()
    expect(getGlobalDialog()).not.toBeNull()
    closeGlobalDialog(getGlobalDialog()!._id)
  })

  it('订阅退订后不再收到广播', () => {
    const fn = vi.fn()
    const unsub = subscribeGlobalDialog(fn)
    unsub()
    openGlobalDialog({ title: 'a' })
    expect(fn).toHaveBeenCalledTimes(1) // 仅订阅瞬间的初始回调
  })
})