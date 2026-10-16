/**
 * useTerminalKeyboardAvoidance 单元测试
 *
 * 覆盖 2026-10-01 改版后的**lift 语义避让**（可移动区域整体上移）契约：
 * ① 双通道检测（visualViewport 优先 / 插件 safeAreaChanged 兜底）+ 10px 阈值
 * ② movableAreaStyle 在键盘可见时给 translateY(-offset)，归零时不留 transform 残值
 * ③ terminalViewStyle **不含 height**（回归锁：防止旧的「根容器高度收缩 = resize
 *    语义」被无意加回——那会让键盘弹收触发重排 + PTY resize）
 * ④ 侧栏输入框聚焦期间禁用避让；⑤ 键盘收起（可见→归零）只回调一次 onKeyboardHide
 *
 * happy-dom 无真实布局与 visualViewport 实现：innerHeight / visualViewport 均以
 * defineProperty 注入，事件走真实 dispatch（safeAreaChanged 是 window 事件）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, nextTick } from 'vue'
import {
  useTerminalKeyboardAvoidance,
  KEYBOARD_VISIBLE_THRESHOLD,
} from '../composables/useTerminalKeyboardAvoidance'

/**
 * 可变 visualViewport 替身：只向**当前注册**的监听器派发（真实事件语义），
 * 因此 dispose() 解绑后的“不再响应”断言是有意义的（而非拿 mock.calls 计数）。
 */
const vvListeners = new Map<string, Set<EventListener>>()
const vv = {
  height: 800,
  addEventListener(type: string, handler: EventListener) {
    if (!vvListeners.has(type)) vvListeners.set(type, new Set())
    vvListeners.get(type)!.add(handler)
  },
  removeEventListener(type: string, handler: EventListener) {
    vvListeners.get(type)?.delete(handler)
  },
}
const LAYOUT_HEIGHT = 800

function setLayoutHeight(h: number) {
  Object.defineProperty(window, 'innerHeight', { configurable: true, writable: true, value: h })
}

/** 改变可视视口高度并派发 resize（等价键盘动画逐帧回调） */
function setViewportHeight(h: number) {
  vv.height = h
  for (const handler of vvListeners.get('resize') ?? []) handler(new Event('resize'))
}

/** 派发插件通道事件（tauri-plugin-edge-to-edge 的 safeAreaChanged） */
function emitPluginKeyboard(keyboardHeight: number, keyboardVisible: boolean) {
  window.dispatchEvent(
    new CustomEvent('safeAreaChanged', { detail: { keyboardHeight, keyboardVisible } }),
  )
}

function createComposable(onKeyboardHide = vi.fn()) {
  return {
    onKeyboardHide,
    api: useTerminalKeyboardAvoidance({
      rootRef: ref(null),
      safeAreaTop: () => 24,
      canvasBackground: () => '#2e3440',
      selectionFrame: () => '#88c0d0',
      onKeyboardHide,
    }),
  }
}

beforeEach(() => {
  vvListeners.clear()
  vv.height = LAYOUT_HEIGHT
  setLayoutHeight(LAYOUT_HEIGHT)
  Object.defineProperty(window, 'visualViewport', { configurable: true, writable: true, value: vv })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('keyboardOffset 双通道检测', () => {
  it('通道1：visualViewport 变矮时偏移 = 布局高 - 可视高（正例）', () => {
    const { api } = createComposable()
    api.attach()
    setViewportHeight(500)
    expect(api.keyboardOffset.value).toBe(300)
  })

  it('通道2：visualViewport 无事件时用插件上报高度兜底（正例）', () => {
    const { api } = createComposable()
    api.attach()
    // 可视视口没变（通道 1 静默），只有插件事件
    emitPluginKeyboard(280, true)
    expect(api.keyboardOffset.value).toBe(280)
  })

  it('通道1 优先：两通道同时有值时取 visualViewport 的实时值，不用插件终态（反例）', () => {
    const { api } = createComposable()
    api.attach()
    emitPluginKeyboard(300, true)
    // 键盘动画中途：插件已报终态 300，可视视口只到 260
    setViewportHeight(LAYOUT_HEIGHT - 260)
    expect(api.keyboardOffset.value).toBe(260)
  })

  it('阈值：偏移不超过 10px 视为无键盘（边界）', () => {
    const { api } = createComposable()
    api.attach()
    setViewportHeight(LAYOUT_HEIGHT - KEYBOARD_VISIBLE_THRESHOLD)
    expect(api.keyboardOffset.value).toBe(0)
  })

  it('插件通道：keyboardVisible=false 时偏移归零（反例）', () => {
    const { api } = createComposable()
    api.attach()
    emitPluginKeyboard(300, true)
    emitPluginKeyboard(0, false)
    expect(api.keyboardOffset.value).toBe(0)
  })

  it('键盘可见期间冻结布局基准：连续 resize 偏移按同一基准累加（不归零）', () => {
    const { api } = createComposable()
    api.attach()
    setViewportHeight(600) // 偏移 200
    setLayoutHeight(700) // 模拟布局视口被改动，基准必须仍为 800
    setViewportHeight(500) // 期望 300 而不是 200
    expect(api.keyboardOffset.value).toBe(300)
  })

  it('侧栏输入框聚焦期间禁用避让（反例）：插件通道有值也不平移', () => {
    const { api } = createComposable()
    api.attach()
    emitPluginKeyboard(300, true)
    api.setSettingsInputFocused(true)
    expect(api.keyboardOffset.value).toBe(0)
    api.setSettingsInputFocused(false)
    expect(api.keyboardOffset.value).toBe(300)
  })
})

describe('movableAreaStyle（lift 语义）', () => {
  it('键盘可见：整块可移动区域按键盘高度上移', () => {
    const { api } = createComposable()
    api.attach()
    setViewportHeight(520)
    expect(api.movableAreaStyle.value).toEqual({ transform: 'translateY(-280px)' })
  })

  it('无键盘：不留 transform 残值（归零后 style 为空对象）', () => {
    const { api } = createComposable()
    api.attach()
    setViewportHeight(520)
    setViewportHeight(LAYOUT_HEIGHT)
    expect(api.movableAreaStyle.value).toEqual({})
  })

  it('terminalViewStyle 只含安全区与主题变量，不含 height（防 resize 语义回接）', () => {
    const { api } = createComposable()
    const style = api.terminalViewStyle.value as Record<string, string>
    expect(style).toEqual({
      paddingTop: '24px',
      '--terminal-canvas-bg': '#2e3440',
      '--terminal-selection-frame': '#88c0d0',
    })
    expect('height' in style).toBe(false)
  })
})

describe('onKeyboardHide 回调', () => {
  it('偏移从可见归零时回调一次（正例）', async () => {
    const { api, onKeyboardHide } = createComposable()
    api.attach()
    setViewportHeight(520)
    await nextTick()
    setViewportHeight(LAYOUT_HEIGHT)
    await nextTick()
    expect(onKeyboardHide).toHaveBeenCalledTimes(1)
  })

  it('阈值内的抖动不触发回调（反例：可见 → 5px → 可见）', async () => {
    const { api, onKeyboardHide } = createComposable()
    api.attach()
    setViewportHeight(LAYOUT_HEIGHT - 300)
    await nextTick()
    setViewportHeight(LAYOUT_HEIGHT - 5) // 归零（≤10）→ 应回调一次
    await nextTick()
    setViewportHeight(LAYOUT_HEIGHT - 300)
    await nextTick()
    setViewportHeight(LAYOUT_HEIGHT - 5)
    await nextTick()
    expect(onKeyboardHide).toHaveBeenCalledTimes(2)
  })

  it('无键盘状态下的普通 resize 不触发回调（反例）', async () => {
    const { api, onKeyboardHide } = createComposable()
    api.attach()
    setViewportHeight(LAYOUT_HEIGHT)
    await nextTick()
    setViewportHeight(790)
    await nextTick()
    expect(onKeyboardHide).not.toHaveBeenCalled()
  })
})

describe('dispose', () => {
  it('卸载后不再响应可视视口事件（监听已解绑）', () => {
    const { api } = createComposable()
    api.attach()
    setViewportHeight(500)
    expect(api.keyboardOffset.value).toBe(300)

    api.dispose()
    setViewportHeight(200)
    // 监听已解绑 → 偏移停在解绑前的值
    expect(api.keyboardOffset.value).toBe(300)
  })

  it('卸载后不再响应插件通道事件（反例）', () => {
    const { api } = createComposable()
    api.attach()
    api.dispose()
    emitPluginKeyboard(300, true)
    expect(api.keyboardOffset.value).toBe(0)
  })
})
