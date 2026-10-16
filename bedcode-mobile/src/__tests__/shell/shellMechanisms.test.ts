/**
 * 壳内机制副本行为契约测试（`src/shell/composables/**` 的平台机制）
 * -----------------------------------------------------------------------------
 * 覆盖四组复制进壳的机制：useSwipeTabs / useToast / usePlatform /
 * useViewportPanGuard。契约来源 = 旧机制实现分支（迁移必须逐条保住行为）。
 *
 * 行为契约：
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-M-01 | useSwipeTabs `onTouchEnd` | 左滑超阈值 → 'left' | onSwitch('left') |
 * | C-M-02 | useSwipeTabs `onTouchEnd` | 右滑超阈值 → 'right' | onSwitch('right') |
 * | C-M-03 | useSwipeTabs 阈值 | 恰好等于 48px 不触发（严格大于） | 不调用 onSwitch |
 * | C-M-04 | useSwipeTabs `onTouchMove` | 垂直主导清零，抬手不切换 | 不调用 onSwitch |
 * | C-M-05 | useToast `typeDispatch` | 类型分发到 sonner 对应方法 | error → toast.error |
 * | C-M-06 | useToast `mapPosition` | bottom → bottom-center，缺省 top-center | 位置实参 |
 * | C-M-07 | useToast 默认时长 | success 3000 / error 5000 / warning 4000 | 实参时长 |
 * | C-M-08 | usePlatform 初始值 | 检测前乐观按移动端（isMobile=true） | 字段断言 |
 * | C-M-09 | usePlatform `simulateForBrowser` | 浏览器默认 desktop → windows/isDesktop | 字段断言 |
 * | C-M-10 | usePlatform 模拟开关 | localStorage platform-mode=mobile → android/isMobile | 字段断言 |
 * | C-M-11 | pan guard 纵向分支 | 链上无滚动容器 → preventDefault | defaultPrevented |
 * | C-M-12 | pan guard 纵向分支 | 链上滚动容器有余量 → 放行 | 未 preventDefault |
 * | C-M-13 | pan guard 横向分支 | 链上有横向滚动容器 → 放行 | 未 preventDefault |
 * | C-M-14 | pan guard 轴锁定 | 位移 < 6px 不锁定方向、不拦截 | 未 preventDefault |
 * | C-M-15 | pan guard `dispose` | 解除监听后不再拦截 | 未 preventDefault |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'

// sonner 是第三方 UI 库边界：按契约替身（返回值形状与真实 toast 一致）
const sonner = vi.hoisted(() => ({
  success: vi.fn((_msg: string, _opts: unknown) => 'id-success'),
  error: vi.fn((_msg: string, _opts: unknown) => 'id-error'),
  warning: vi.fn((_msg: string, _opts: unknown) => 'id-warning'),
  info: vi.fn((_msg: string, _opts: unknown) => 'id-info'),
  dismiss: vi.fn(),
}))

vi.mock('vue-sonner', () => ({ toast: sonner }))

import { useToast } from '@/shell/composables/useToast'
import { useSwipeTabs } from '@/shell/composables/useSwipeTabs'

// ==================== useToast ====================

describe('useToast（壳内机制）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should_dispatchToMatchingSonnerMethod_when_typeGiven', () => {
    const toast = useToast()
    toast.show({ message: '出错了', type: 'error' })
    toast.show({ message: '好了', type: 'success' })
    toast.show({ message: '注意', type: 'warning' })
    toast.show({ message: '提示' })

    expect(sonner.error).toHaveBeenCalledTimes(1)
    expect(sonner.success).toHaveBeenCalledTimes(1)
    expect(sonner.warning).toHaveBeenCalledTimes(1)
    // 未指定类型按 info 兜底
    expect(sonner.info).toHaveBeenCalledTimes(1)
    expect(sonner.info).toHaveBeenCalledWith('提示', expect.objectContaining({ position: 'top-center' }))
  })

  it('should_mapPositionAndDefaultDurations_when_messageShown', () => {
    const toast = useToast()

    toast.show({ message: '底部', type: 'info', position: 'bottom' })
    expect(sonner.info).toHaveBeenCalledWith('底部', {
      duration: undefined,
      position: 'bottom-center',
    })

    sonner.success.mockClear()
    toast.success('成功')
    expect(sonner.success).toHaveBeenCalledWith('成功', {
      duration: 3000,
      position: 'top-center',
    })

    sonner.error.mockClear()
    toast.error('失败')
    expect(sonner.error).toHaveBeenCalledWith('失败', {
      duration: 5000,
      position: 'top-center',
    })

    sonner.warning.mockClear()
    toast.warning('警告')
    expect(sonner.warning).toHaveBeenCalledWith('警告', {
      duration: 4000,
      position: 'top-center',
    })
  })

  it('should_dismissSpecificAndAll_when_called', () => {
    const toast = useToast()
    toast.dismiss('id-1')
    expect(sonner.dismiss).toHaveBeenCalledWith('id-1')
    toast.dismissAll()
    expect(sonner.dismiss).toHaveBeenCalledWith()
  })
})

// ==================== useSwipeTabs ====================

describe('useSwipeTabs（壳内机制）', () => {
  /** 触摸点替身：机制只读 clientX / clientY */
  function touch(x: number, y: number): Touch {
    return { clientX: x, clientY: y } as Touch
  }

  function swipe(onSwitch: (dir: 'left' | 'right') => void, points: Array<[number, number]>) {
    const handlers = useSwipeTabs(onSwitch)
    handlers.onTouchStart({ touches: [touch(points[0][0], points[0][1])] } as unknown as TouchEvent)
    for (const [x, y] of points.slice(1)) {
      handlers.onTouchMove({ touches: [touch(x, y)] } as unknown as TouchEvent)
    }
    handlers.onTouchEnd()
  }

  it('should_switchLeftAndRight_when_horizontalSwipeExceedsThreshold', () => {
    const left = vi.fn()
    swipe(left, [[200, 100], [149, 100]])
    expect(left).toHaveBeenCalledWith('left')

    const right = vi.fn()
    swipe(right, [[100, 100], [151, 100]])
    expect(right).toHaveBeenCalledWith('right')
  })

  it('should_notSwitch_when_displacementIsExactlyThreshold', () => {
    const left = vi.fn()
    // 阈值 48 为严格比较：恰好 48px 不触发（点按/轻微移动容差）
    swipe(left, [[200, 100], [152, 100]])
    expect(left).not.toHaveBeenCalled()

    const right = vi.fn()
    swipe(right, [[100, 100], [148, 100]])
    expect(right).not.toHaveBeenCalled()
  })

  it('should_notSwitch_when_verticalDominates', () => {
    const onSwitch = vi.fn()
    // 纵向位移更大：deltaX 清零，抬手不切换（不干扰内容区滚动）
    swipe(onSwitch, [[200, 100], [160, 300]])
    expect(onSwitch).not.toHaveBeenCalled()
  })

  it('should_notSwitch_when_touchEndsWithoutMove', () => {
    const onSwitch = vi.fn()
    swipe(onSwitch, [[100, 100]])
    expect(onSwitch).not.toHaveBeenCalled()
  })
})

// ==================== usePlatform ====================

describe('usePlatform（壳内机制）', () => {
  beforeEach(() => {
    vi.resetModules()
    localStorage.clear()
    delete (window as unknown as Record<string, unknown>).__TAURI__
  })

  afterEach(() => {
    localStorage.clear()
  })

  it('should_exposeOptimisticMobileDefault_when_notDetectedYet', async () => {
    const { getPlatformInfo } = await import('@/shell/composables/usePlatform')
    const info = getPlatformInfo()
    expect(info.platform).toBeNull()
    expect(info.isMobile).toBe(true)
    expect(info.isDesktop).toBe(false)
  })

  it('should_simulateDesktopByDefault_when_browserRuntime', async () => {
    const { initPlatform, getPlatformInfo } = await import('@/shell/composables/usePlatform')
    const info = await initPlatform()

    expect(info.platform).toBe('windows')
    expect(info.isDesktop).toBe(true)
    expect(info.isMobile).toBe(false)
    expect(info.osType).toBe('Web')
    // 单例：initPlatform 写入后 getPlatformInfo 必须看到同一份事实
    expect(getPlatformInfo().platform).toBe(info.platform)
  })

  it('should_simulateMobile_when_platformModeStored', async () => {
    localStorage.setItem('platform-mode', 'mobile')
    const { initPlatform, getPlatformInfo } = await import('@/shell/composables/usePlatform')
    await initPlatform()

    const info = getPlatformInfo()
    expect(info.platform).toBe('android')
    expect(info.isMobile).toBe(true)
    expect(info.isAndroid).toBe(true)
    expect(info.isDesktop).toBe(false)
  })

  it('should_detectMobileInComponentContext_when_modeStored', async () => {
    localStorage.setItem('platform-mode', 'mobile')
    const { useIsMobile } = await import('@/shell/composables/usePlatform')
    const Probe = defineComponent({
      setup() {
        const isMobile = useIsMobile()
        return () => h('span', isMobile.value ? 'mobile' : 'desktop')
      },
    })
    const wrapper = mount(Probe)
    // 探测是异步的（plugin-os 动态 import + onMounted）：等状态真正落到视图
    await vi.waitFor(() => expect(wrapper.text()).toBe('mobile'))
  })
})

// ==================== useViewportPanGuard ====================

describe('useViewportPanGuard（壳内机制）', () => {
  /** happy-dom 不解析内联 overflow：按元素注册期望值（与旧机制测试同法） */
  const overflowStyles = new Map<Element, { overflowX?: string; overflowY?: string }>()
  const styleSpy = vi.spyOn(window, 'getComputedStyle').mockImplementation((el: Element) => ({
    overflowX: overflowStyles.get(el)?.overflowX ?? 'visible',
    overflowY: overflowStyles.get(el)?.overflowY ?? 'visible',
  }) as CSSStyleDeclaration)

  beforeEach(() => {
    overflowStyles.clear()
    document.body.innerHTML = ''
  })

  afterEach(() => {
    overflowStyles.clear()
    styleSpy.mockClear()
  })

  function makeEl(opts: {
    overflowY?: string
    overflowX?: string
    scrollTop?: number
    scrollHeight?: number
    clientHeight?: number
  } = {}): HTMLElement {
    const el = document.createElement('div')
    overflowStyles.set(el, { overflowY: opts.overflowY, overflowX: opts.overflowX })
    const define = (prop: string, value: number) => {
      Object.defineProperty(el, prop, { configurable: true, value })
    }
    define('scrollTop', opts.scrollTop ?? 0)
    define('scrollHeight', opts.scrollHeight ?? 100)
    define('clientHeight', opts.clientHeight ?? 100)
    return el
  }

  /** 构造可派发的触摸事件（happy-dom 无 TouchEvent 构造器） */
  function touchEvent(type: string, x: number, y: number): Event {
    const event = new Event(type, { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'touches', { value: [{ clientX: x, clientY: y }] })
    return event
  }

  it('should_preventDefault_when_verticalGestureHasNoScrollableChain', async () => {
    const { attachViewportPanGuard } = await import('@/shell/composables/useViewportPanGuard')
    const root = makeEl()
    const target = makeEl()
    root.appendChild(target)
    document.body.appendChild(root)
    attachViewportPanGuard(root)

    target.dispatchEvent(touchEvent('touchstart', 100, 100))
    const move = touchEvent('touchmove', 100, 60)
    target.dispatchEvent(move)
    expect(move.defaultPrevented).toBe(true)
  })

  it('should_allowVerticalGesture_when_chainScrollerHasRoom', async () => {
    const { attachViewportPanGuard } = await import('@/shell/composables/useViewportPanGuard')
    const root = makeEl()
    const scroller = makeEl({ overflowY: 'auto', scrollTop: 0, scrollHeight: 500, clientHeight: 100 })
    const target = makeEl()
    root.appendChild(scroller)
    scroller.appendChild(target)
    document.body.appendChild(root)
    attachViewportPanGuard(root)

    target.dispatchEvent(touchEvent('touchstart', 100, 100))
    const move = touchEvent('touchmove', 100, 60)
    target.dispatchEvent(move)
    expect(move.defaultPrevented).toBe(false)
  })

  it('should_allowHorizontalGesture_when_horizontalScrollerPresent', async () => {
    const { attachViewportPanGuard } = await import('@/shell/composables/useViewportPanGuard')
    const root = makeEl()
    const quickBar = makeEl({ overflowX: 'auto' })
    const target = makeEl()
    root.appendChild(quickBar)
    quickBar.appendChild(target)
    document.body.appendChild(root)
    attachViewportPanGuard(root)

    target.dispatchEvent(touchEvent('touchstart', 100, 100))
    const move = touchEvent('touchmove', 160, 102)
    target.dispatchEvent(move)
    expect(move.defaultPrevented).toBe(false)
  })

  it('should_notPreventDefault_when_displacementBelowAxisThreshold', async () => {
    const { attachViewportPanGuard } = await import('@/shell/composables/useViewportPanGuard')
    const root = makeEl()
    const target = makeEl()
    root.appendChild(target)
    document.body.appendChild(root)
    attachViewportPanGuard(root)

    target.dispatchEvent(touchEvent('touchstart', 100, 100))
    const move = touchEvent('touchmove', 104, 96)
    target.dispatchEvent(move)
    expect(move.defaultPrevented).toBe(false)
  })

  it('should_stopIntercepting_when_disposed', async () => {
    const { attachViewportPanGuard } = await import('@/shell/composables/useViewportPanGuard')
    const root = makeEl()
    const target = makeEl()
    root.appendChild(target)
    document.body.appendChild(root)
    const guard = attachViewportPanGuard(root)
    guard.dispose()

    target.dispatchEvent(touchEvent('touchstart', 100, 100))
    const move = touchEvent('touchmove', 100, 60)
    target.dispatchEvent(move)
    expect(move.defaultPrevented).toBe(false)
  })
})
