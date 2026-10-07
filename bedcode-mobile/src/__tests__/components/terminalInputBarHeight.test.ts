/**
 * TerminalInputBar 输入框高度行为契约测试
 *
 * 需求（用户报告 bug）：输入多行字符后发送/执行成功，输入框显示高度应回落
 * 单行（高度与当前实际显示的字符行数强相关），而不是停留在多行高度。
 *
 * 实现机制：textarea 显示高度由 watch(inputText) 驱动的 adjustTextareaHeight()
 * 按内容换行数设置（1~6 行）。本测试模拟真实浏览器行为——scrollHeight 随
 * textarea.value 的换行数线性增长——验证：
 *  - 多行输入 → 高度增高（1 行 → N 行）
 *  - 执行/发送成功清空输入 → 高度回落单行
 *  - 空输入不发送、高度恒单行
 *  - 回落后再输入 → 高度重新跟随（watcher 未被残留高度破坏）
 *
 * 注意：happy-dom 无布局引擎，scrollHeight（Element.prototype 上的 getter）
 * 恒为 0。测试在实例上定义 shadow getter 替身：21px = 1 行（与组件内
 * lineHeight 兜底一致），随 value 换行数线性返回。
 */
import { describe, it, expect, vi } from 'vitest'
import { ref, nextTick } from 'vue'
import { mount } from '@vue/test-utils'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ error: vi.fn(), warning: vi.fn() }),
}))

vi.mock('@/composables/useMobileSettings', () => ({
  useMobileSettings: () => ({
    settings: { value: { vibrate: false } },
  }),
}))

vi.mock('@/stores/inputAssistant', () => ({
  useInputAssistantStore: () => ({
    presetCommands: [],
    visiblePanelShortcuts: [],
    shortcutConfig: [],
    getQuickBarItems: () => [],
    recordShortcut: vi.fn(),
    recordCustomCommand: vi.fn(),
  }),
}))

import TerminalInputBar from '@/components/TerminalInputBar.vue'

const LINE_HEIGHT = 21

/** 在 textarea 实例上挂 scrollHeight 替身（shadow 原型 getter）：行数 × 行高 */
function mockScrollHeight(textarea: HTMLTextAreaElement) {
  Object.defineProperty(textarea, 'scrollHeight', {
    configurable: true,
    get() {
      const value = textarea.value
      const lines = value ? value.split('\n').length : 1
      return Math.max(lines, 1) * LINE_HEIGHT
    },
  })
}

// ==================== 夹具 ====================

function mountInputBar() {
  const wrapper = mount(TerminalInputBar, {
    attachTo: document.body,
    global: {
      provide: {
        safeArea: ref({ top: 0, bottom: 0, navigationBar: 0 }),
      },
    },
  })
  mockScrollHeight(wrapper.find('textarea').element as HTMLTextAreaElement)
  return wrapper
}

/** 设置输入框内容（等价真实输入：写 DOM value + 派发 input 事件走 v-model） */
async function typeText(wrapper: ReturnType<typeof mount>, text: string) {
  const textarea = wrapper.find('textarea')
  ;(textarea.element as HTMLTextAreaElement).value = text
  await textarea.trigger('input')
  await nextTick()
}

/** 点击执行按钮（pointerdown → pointerup 短按）。组件用 setPointerCapture?. 可选链，
 *  happy-dom 无该方法 → 自动跳过，无需替身 */
async function clickExecute(wrapper: ReturnType<typeof mount>) {
  const btn = wrapper.find('.execute-btn')
  await btn.trigger('pointerdown')
  await btn.trigger('pointerup')
  await nextTick()
}

// ==================== 行为契约 ====================

describe('TerminalInputBar 输入框高度与内容行数强相关', () => {
  it('多行输入 → 显示高度随行数增高（1 行 21px → 3 行 63px）', async () => {
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    expect(textarea.element.style.height).toBe('')

    await typeText(wrapper, 'line1\nline2\nline3')
    expect(textarea.element.style.height).toBe(`${3 * LINE_HEIGHT}px`)
    wrapper.unmount()
  })

  it('多行输入后执行成功 → 输入框清空且高度回落单行（21px）', async () => {
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    await typeText(wrapper, 'line1\nline2\nline3')
    expect(textarea.element.style.height).toBe(`${3 * LINE_HEIGHT}px`)

    await clickExecute(wrapper)
    // 执行语义：文本（+ enter）发到终端
    expect(wrapper.emitted('execute')).toEqual([['line1\nline2\nline3']])
    // 输入框内容清空
    expect((textarea.element as HTMLTextAreaElement).value).toBe('')
    // 高度回落单行（本测试针对的 bug：此前停留在 63px 不回落）
    expect(textarea.element.style.height).toBe(`${LINE_HEIGHT}px`)
    wrapper.unmount()
  })

  it('多行输入后发送（send 模式）成功 → 高度回落单行', async () => {
    vi.useFakeTimers()
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    await typeText(wrapper, 'a\nb')
    expect(textarea.element.style.height).toBe(`${2 * LINE_HEIGHT}px`)

    // 长按 400ms 切换为发送模式（sendMode=true），松开不触发动作
    const btn = wrapper.find('.execute-btn')
    await btn.trigger('pointerdown')
    vi.advanceTimersByTime(410)
    await btn.trigger('pointerup')

    await typeText(wrapper, 'a\nb\nc')
    expect(textarea.element.style.height).toBe(`${3 * LINE_HEIGHT}px`)

    await clickExecute(wrapper)
    expect(wrapper.emitted('submit')).toEqual([['a\nb\nc']])
    expect((textarea.element as HTMLTextAreaElement).value).toBe('')
    expect(textarea.element.style.height).toBe(`${LINE_HEIGHT}px`)
    wrapper.unmount()
    vi.useRealTimers()
  })

  it('空输入 → 不发送、高度保持初始（无内联高度）', async () => {
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    await clickExecute(wrapper)
    expect(wrapper.emitted('execute')).toBeUndefined()
    expect(wrapper.emitted('submit')).toBeUndefined()
    expect(textarea.element.style.height).toBe('')
    wrapper.unmount()
  })

  it('单行输入执行 → 高度保持单行，回落逻辑不误伤', async () => {
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    await typeText(wrapper, 'ls -la')
    expect(textarea.element.style.height).toBe(`${LINE_HEIGHT}px`)

    await clickExecute(wrapper)
    expect(wrapper.emitted('execute')).toEqual([['ls -la']])
    expect((textarea.element as HTMLTextAreaElement).value).toBe('')
    expect(textarea.element.style.height).toBe(`${LINE_HEIGHT}px`)
    wrapper.unmount()
  })

  it('回落后再输入多行 → 高度重新增高（watcher 未被残留高度破坏）', async () => {
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    await typeText(wrapper, 'x\ny\nz')
    await clickExecute(wrapper)
    expect(textarea.element.style.height).toBe(`${LINE_HEIGHT}px`)

    await typeText(wrapper, 'p\nq')
    expect(textarea.element.style.height).toBe(`${2 * LINE_HEIGHT}px`)
    wrapper.unmount()
  })
})

describe('TerminalInputBar `/` 命令补全与 watcher 复位的互动', () => {
  it('点选补全项后补全面板保持关闭（applyCompletion 覆盖 watcher 复位）', async () => {
    const wrapper = mountInputBar()
    const textarea = wrapper.find('textarea')
    // 聚焦 + 输入 /c → 匹配 /clear /compact /context 等 → 补全面板出现
    await textarea.trigger('focus')
    await typeText(wrapper, '/c')
    expect(wrapper.find('.completion-panel').exists()).toBe(true)

    // 点选首个补全项（/clear）：整体填充输入框并关闭面板
    await wrapper.find('.completion-item').trigger('click')
    await nextTick()
    // 点击后面板必须保持关闭（等下一次真实输入再出现）——
    // watcher 复位 completionDismissed 不能晚于 applyCompletion 的置位
    expect(wrapper.find('.completion-panel').exists()).toBe(false)
    expect((textarea.element as HTMLTextAreaElement).value).toBe('/clear')
    wrapper.unmount()
  })
})
