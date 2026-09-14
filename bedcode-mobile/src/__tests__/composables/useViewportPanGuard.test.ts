/**
 * useViewportPanGuard 单元测试
 *
 * 覆盖：纵向滚动容器方向余量判定、横向滚动容器存在性（快捷条 RTL 场景
 * 刻意不做余量检查）、链式遍历在守卫根节点处终止。
 * happy-dom 无真实布局，滚动量与 overflow 以属性/样式覆写注入。
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import {
  nativeScrollCanConsume,
  hasNativeHorizontalScroller,
  verticalChainCanConsume,
} from '@/composables/useViewportPanGuard'

// happy-dom 的 getComputedStyle 不解析 overflow 内联样式：按元素注册期望值
const overflowStyles = new Map<Element, { overflowX?: string; overflowY?: string }>()
vi.spyOn(window, 'getComputedStyle').mockImplementation((el: Element) => ({
  overflowX: overflowStyles.get(el)?.overflowX ?? 'visible',
  overflowY: overflowStyles.get(el)?.overflowY ?? 'visible',
}) as CSSStyleDeclaration)
afterEach(() => overflowStyles.clear())

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

/** 组装「根 → 中间 → 触点」的祖先链（从外到内依次嵌套），返回触点 */
function nestChain(root: HTMLElement, middle: HTMLElement, target: HTMLElement): HTMLElement {
  root.appendChild(middle)
  middle.appendChild(target)
  return target
}

describe('nativeScrollCanConsume', () => {
  it('allows vertical scroll when room remains below (finger up)', () => {
    const el = makeEl({ overflowY: 'auto', scrollTop: 0, scrollHeight: 500, clientHeight: 100 })
    expect(nativeScrollCanConsume(el, -10)).toBe(true)
  })

  it('blocks when scrolled to the bottom (finger up, no room)', () => {
    const el = makeEl({ overflowY: 'auto', scrollTop: 400, scrollHeight: 500, clientHeight: 100 })
    expect(nativeScrollCanConsume(el, -10)).toBe(false)
  })

  it('allows scrolling up when room remains above (finger down)', () => {
    const el = makeEl({ overflowY: 'auto', scrollTop: 100, scrollHeight: 500, clientHeight: 100 })
    expect(nativeScrollCanConsume(el, 10)).toBe(true)
  })

  it('blocks non-scrollable containers (overflow hidden/visible)', () => {
    const hidden = makeEl({ overflowY: 'hidden', scrollHeight: 500, clientHeight: 100 })
    const visible = makeEl({ scrollHeight: 500, clientHeight: 100 })
    expect(nativeScrollCanConsume(hidden, -10)).toBe(false)
    expect(nativeScrollCanConsume(visible, -10)).toBe(false)
  })
})

describe('hasNativeHorizontalScroller', () => {
  it('finds horizontal scroller anywhere in the chain below the guard root', () => {
    // 模拟 quick-bar：守卫根 → overflow-x auto 容器 → 触点按钮
    const root = makeEl()
    const btn = nestChain(root, makeEl({ overflowX: 'auto' }), makeEl())
    expect(hasNativeHorizontalScroller(btn, root)).toBe(true)
  })

  it('returns false when no horizontal scroller in the chain', () => {
    const root = makeEl()
    const btn = nestChain(root, makeEl({ overflowX: 'hidden' }), makeEl())
    expect(hasNativeHorizontalScroller(btn, root)).toBe(false)
  })

  it('stops the walk at the guard root', () => {
    // 滚动容器在守卫根之外（含根自身）：不属于放行范围
    const root = makeEl({ overflowX: 'auto' })
    const btn = nestChain(root, makeEl(), makeEl())
    expect(hasNativeHorizontalScroller(btn, root)).toBe(false)
  })
})

describe('verticalChainCanConsume', () => {
  it('allows when a chain scroller can consume the vertical gesture', () => {
    const root = makeEl()
    const target = nestChain(
      root,
      makeEl({ overflowY: 'auto', scrollTop: 0, scrollHeight: 500, clientHeight: 100 }),
      makeEl(),
    )
    expect(verticalChainCanConsume(target, root, -10)).toBe(true)
  })

  it('blocks when chain scrollers are at their edge (page pan protection)', () => {
    const root = makeEl()
    const target = nestChain(
      root,
      makeEl({ overflowY: 'auto', scrollTop: 400, scrollHeight: 500, clientHeight: 100 }),
      makeEl(),
    )
    expect(verticalChainCanConsume(target, root, -10)).toBe(false)
  })
})
