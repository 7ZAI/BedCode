/**
 * useTerminalScroll 单元测试
 *
 * 覆盖重构核心逻辑（对齐桌面端滚动语义）：
 * - onScroll 位置推导 isUserScrolling（底部 → 自动跟随；向上滚 → 停止跟随）
 * - touchActive 触摸滚动锁：手指按下期间 scrollToBottom 被忽略，抬起后恢复
 * - scrollToBottomManual 显式复位滚动锁
 * - handleShortcutsPanelToggle 仅在底部时上移内容（间接验证 isAtBottom 推导）
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { nextTick, ref } from 'vue'
import type { Terminal } from '@xterm/xterm'

// 依赖替身：toast / clipboard 与滚动逻辑无关
vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() }),
}))
vi.mock('@/utils/clipboard', () => ({
  writeClipboardText: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@/locales', () => ({ default: { global: { t: (k: string) => k } } }))

import { useTerminalScroll } from '@/composables/useTerminalScroll'

/** 构造 xterm mock：可变 viewportY / bufferLength，捕获 onScroll 回调 */
function makeMockTerminal(viewportY: number, bufferLength: number, rows: number) {
  const state = { viewportY, bufferLength }
  const onScrollCb = { cb: null as ((y: number) => void) | null }
  const term = {
    rows,
    cols: 20,
    element: document.createElement('div'),
    scrollToLine: vi.fn(),
    refresh: vi.fn(),
    onLineFeed: vi.fn(() => ({ dispose: vi.fn() })),
    onScroll: vi.fn((cb: (y: number) => void) => {
      onScrollCb.cb = cb
      return { dispose: vi.fn() }
    }),
    onResize: vi.fn(() => ({ dispose: vi.fn() })),
    buffer: {
      active: {
        get viewportY() {
          return state.viewportY
        },
        get length() {
          return state.bufferLength
        },
      },
    },
  }
  return { term, state, onScrollCb }
}

describe('useTerminalScroll', () => {
  // rAF mock：记录回调、支持 cancel 语义、可手动推进帧
  let rafMap: Map<number, FrameRequestCallback>
  let rafId: number

  beforeEach(() => {
    rafMap = new Map()
    rafId = 0
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      rafMap.set(++rafId, cb)
      return rafId
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      rafMap.delete(id)
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  /** 执行当前挂起的所有 rAF 回调（模拟帧结束）；循环直到无新调度（滚动会追加重绘） */
  function flushFrames() {
    let guard = 0
    while (rafMap.size > 0 && guard++ < 100) {
      const cbs = [...rafMap.values()]
      rafMap.clear()
      for (const cb of cbs) cb(0)
    }
  }

  async function setup(viewportY = 0, bufferLength = 10, rows = 5) {
    const { term, state, onScrollCb } = makeMockTerminal(viewportY, bufferLength, rows)
    const terminalRef = ref<Terminal | null>(null)
    const container = document.createElement('div')
    const scrollContainerRef = ref<HTMLDivElement | null>(container)
    const scroll = useTerminalScroll(terminalRef, scrollContainerRef)
    terminalRef.value = term
    scroll.setupViewportScroll()
    await nextTick() // setupViewportScroll 内部 nextTick(scrollToBottom)
    flushFrames() // 执行初始滚动调度，保证后续计数干净
    return { term, state, onScrollCb, container, scroll }
  }

  /** 派发触摸事件（jsdom 无 TouchEvent，用 defineProperty 补 touches） */
  function dispatchTouch(container: HTMLElement, type: 'touchstart' | 'touchend', y = 100) {
    const ev = new Event(type, { bubbles: true })
    Object.defineProperty(ev, 'touches', {
      value: type === 'touchstart' ? [{ clientX: 10, clientY: y }] : [],
    })
    container.dispatchEvent(ev)
  }

  it('onScroll 位置推导：底部 → 自动跟随；向上滚 → 停止跟随', async () => {
    const { state, onScrollCb, scroll } = await setup(0, 10, 5)

    // 底部（viewportY=5 即 buffer 底）→ isUserScrolling=false
    state.viewportY = 5
    onScrollCb.cb!(5)
    expect(scroll.isUserScrolling.value).toBe(false)

    // 向上滚动（viewportY=2）→ isUserScrolling=true
    state.viewportY = 2
    onScrollCb.cb!(2)
    expect(scroll.isUserScrolling.value).toBe(true)
  })

  it('触摸滚动锁：按下期间 scrollToBottom 被忽略，抬起后恢复', async () => {
    const { container, scroll } = await setup()

    // 无触摸：scrollToBottom 调度 rAF（帧推进后清空）
    scroll.scrollToBottom()
    expect(rafMap.size).toBe(1)
    flushFrames()
    expect(rafMap.size).toBe(0)

    // 手指按下 → 触摸锁生效 → 不调度（同帧节流已清空，此处应无任何调度）
    dispatchTouch(container, 'touchstart')
    scroll.scrollToBottom()
    expect(rafMap.size).toBe(0)

    // 手指抬起 → 锁解除 → 恢复调度
    dispatchTouch(container, 'touchend')
    scroll.scrollToBottom()
    expect(rafMap.size).toBe(1)
  })

  it('scrollToBottomManual 显式复位触摸锁并立即滚到底', async () => {
    const { container, scroll, term } = await setup()

    // 模拟手指按住（触摸锁生效）
    dispatchTouch(container, 'touchstart')
    term.scrollToLine.mockClear()

    scroll.scrollToBottomManual()
    // 复位后立即滚到底：scrollToLine 直接调用（不依赖触摸锁、不走 rAF）
    expect(term.scrollToLine).toHaveBeenCalledWith(5) // 10 行 buffer - 5 行视口
    // 滚动由 xterm 内部自带重绘，无额外 rAF 调度
    expect(rafMap.size).toBe(0)

    // 手动复位后触摸锁解除：scrollToBottom 恢复调度
    scroll.scrollToBottom()
    expect(rafMap.size).toBe(1)
  })

  it('输出自动跟随的追赶滚动不污染 isUserScrolling（回放不误判用户上滚）', async () => {
    const { state, onScrollCb, scroll } = await setup(0, 10, 5)

    // 回放流式写入触发自动跟随：scrollToBottom 调度 rAF → scrollToLine
    //（真实 xterm 同步 fire onScroll，mock 下手动模拟该次滚动事件）
    scroll.scrollToBottom()
    flushFrames()
    state.viewportY = 0
    onScrollCb.cb!(0) // 追赶滚动触发的 onScroll：应跳过推导
    expect(scroll.isUserScrolling.value).toBe(false)

    // 追赶完成后用户真实上滚（非自动跟随）：位置推导正常生效
    state.viewportY = 2
    onScrollCb.cb!(2)
    expect(scroll.isUserScrolling.value).toBe(true)

    // 上滚后新输出到达：scrollToBottom 被守卫拦截（不把用户拉回底部）
    scroll.scrollToBottom()
    expect(rafMap.size).toBe(0)
  })

  it('触摸接管后清除自动跟随标志（后续推导从干净状态开始）', async () => {
    const { state, onScrollCb, scroll, container } = await setup(0, 10, 5)

    // 自动跟随滚动执行（scrollToLine 未触发 onScroll 的边界：标志残留）
    scroll.scrollToBottom()
    flushFrames()

    // 手指按下：清除残留标志
    dispatchTouch(container, 'touchstart')

    // 抬起后触发的推导不应被残留标志跳过
    state.viewportY = 2
    dispatchTouch(container, 'touchend')
    onScrollCb.cb!(2)
    expect(scroll.isUserScrolling.value).toBe(true)
  })

  it('handleShortcutsPanelToggle：仅在底部时上移内容（isAtBottom 推导）', async () => {
    const { state, onScrollCb, scroll } = await setup(5, 10, 5) // 初始在底部

    // 在底部 → 设置面板高度
    scroll.handleShortcutsPanelToggle(120)
    expect(scroll.shortcutsPanelHeight.value).toBe(120)

    // 向上滚动后 → 忽略面板高度（不遮当前行）
    state.viewportY = 1
    onScrollCb.cb!(1)
    scroll.handleShortcutsPanelToggle(120)
    expect(scroll.shortcutsPanelHeight.value).toBe(120) // 保持旧值，不覆盖

    // 面板收起 → 归零
    scroll.handleShortcutsPanelToggle(0)
    expect(scroll.shortcutsPanelHeight.value).toBe(0)
  })
})
