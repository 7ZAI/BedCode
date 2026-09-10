import { describe, it, expect, afterEach, vi } from 'vitest'
import { applySelectPanelPosition } from '../src/ui/apply-select-position'
import { resetFixedZoomProbe } from '../src/ui/zoom-compensation'

/**
 * applySelectPanelPosition DOM 定位封装单测
 *
 * happy-dom 无布局引擎，按元素分派 mock getBoundingClientRect 模拟
 * WebKitGTK 根 zoom 1.15 的实测坐标语义（见 zoom-compensation.ts）：
 * - gBCR 读数 = 设计值 × F（视觉坐标）
 * - 写回 px 渲染 = 设计值 × F
 * 断言基于设计空间期望值（写回 px 即设计 px）。
 */

interface RectInit {
  top?: number
  bottom?: number
  left?: number
  width?: number
  height?: number
}

function domRect(r: RectInit = {}): DOMRect {
  const top = r.top ?? 0
  const left = r.left ?? 0
  const width = r.width ?? 0
  const height = r.height ?? 0
  return {
    top,
    left,
    width,
    height,
    bottom: r.bottom ?? top + height,
    right: r.left !== undefined && r.width !== undefined ? left + width : (r.right ?? 0),
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect
}

function mountFixture(): { trigger: HTMLElement; panel: HTMLElement; list: HTMLElement } {
  const trigger = document.createElement('div')
  const panel = document.createElement('div')
  const list = document.createElement('ul')
  panel.appendChild(list)
  document.body.appendChild(trigger)
  document.body.appendChild(panel)
  return { trigger, panel, list }
}

/**
 * 按元素分派 gBCR mock：
 * - zoom-compensation 探针（inline style position:fixed + width:100px）→ probeWidth
 * - trigger / panel → 指定视觉矩形；panelHeight 传函数时按当前 list 约束动态取值
 */
function mockRects(opts: {
  probeWidth: number
  trigger: RectInit
  panelHeight: number | ((list: HTMLElement) => number)
  fixture: { trigger: HTMLElement; panel: HTMLElement; list: HTMLElement }
}) {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: HTMLElement,
  ) {
    if (this.style.position === 'fixed' && this.style.width === '100px') {
      return domRect({ width: opts.probeWidth })
    }
    if (this === opts.fixture.trigger) return domRect(opts.trigger)
    if (this === opts.fixture.panel) {
      const h =
        typeof opts.panelHeight === 'function'
          ? opts.panelHeight(opts.fixture.list)
          : opts.panelHeight
      return domRect({ height: h, width: opts.trigger.width })
    }
    return domRect()
  })
}

const VIEWPORT = { w: 1100, h: 750 } // 视觉视口（F=1.15 时设计空间 956.5 × 652.2）

/** 断言写回 px（÷F 换算与 mock 视觉值存在亚像素尾差，按 0.05px 近似比较） */
function expectPx(el: HTMLElement, prop: 'top' | 'left' | 'width', design: number) {
  expect(parseFloat(el.style[prop])).toBeCloseTo(design, 1)
}
function expectMaxHeight(list: HTMLElement, design: number) {
  expect(parseFloat(list.style.maxHeight)).toBeCloseTo(design, 1)
}

afterEach(() => {
  resetFixedZoomProbe()
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

describe('applySelectPanelPosition', () => {
  it('F=1（无 zoom）：读数即设计值，行为与非缩放端逐位一致', () => {
    const fixture = mountFixture()
    mockRects({
      probeWidth: 0, // 无布局引擎/无 zoom → F 退回 1
      trigger: { top: 400, bottom: 432, left: 100, width: 200 },
      panelHeight: 240,
      fixture,
    })
    window.innerWidth = VIEWPORT.w
    window.innerHeight = VIEWPORT.h

    applySelectPanelPosition(fixture)

    // 下方空间 750-432-4=314 ≥ 240 → 向下展开，写回原值
    expect(fixture.panel.style.top).toBe('436px')
    expect(fixture.panel.style.left).toBe('100px')
    expect(fixture.panel.style.width).toBe('200px')
    expect(fixture.list.style.maxHeight).toBe('240px')
  })

  it('F=1.15（Linux 根 zoom）：下方不足上方充足 → 上翻，写回设计坐标', () => {
    const fixture = mountFixture()
    mockRects({
      probeWidth: 115, // 探针 100px 读回 115px → F=1.15
      // 触发器设计位置 top 480（视觉 552），设计高 36（视觉 41.39）
      trigger: { top: 552, bottom: 593.390625, left: 138, width: 253 },
      panelHeight: 460, // 自然高 400 设计（视觉 460）
      fixture,
    })
    window.innerWidth = VIEWPORT.w
    window.innerHeight = VIEWPORT.h

    applySelectPanelPosition(fixture)

    const f = 1.15
    const topDesign = 480 // 触发器 top(视觉 552)÷F
    const belowDesign = 750 / f - 516 - 4 // < 240，放不下
    expect(belowDesign).toBeLessThan(240)
    // 上翻：top = 触发器设计 top - 4 - 240（自然高 400 收缩到设计上限 240）
    expectPx(fixture.panel, 'top', topDesign - 4 - 240)
    expectPx(fixture.panel, 'left', 138 / f) // 120 设计
    expectPx(fixture.panel, 'width', 253 / f) // 220 设计
    expectMaxHeight(fixture.list, 240)
  })

  it('F=1.15：下方空间充足 → 向下展开', () => {
    const fixture = mountFixture()
    mockRects({
      probeWidth: 115,
      trigger: { top: 230, bottom: 271.390625, left: 138, width: 253 }, // 设计 top 200
      panelHeight: 460,
      fixture,
    })
    window.innerWidth = VIEWPORT.w
    window.innerHeight = VIEWPORT.h

    applySelectPanelPosition(fixture)

    // 下方空间 652.2-236-4=412 ≥ 240 → top = 触发器设计 bottom 236 + 4
    expectPx(fixture.panel, 'top', 240)
    expectMaxHeight(fixture.list, 240)
  })

  it('陈旧 maxHeight 残留：测量前解除约束，按自然高判定翻转', () => {
    const fixture = mountFixture()
    fixture.list.style.maxHeight = '50px' // 模拟上次收缩残留
    mockRects({
      probeWidth: 115,
      trigger: { top: 552, bottom: 593.390625, left: 138, width: 253 },
      // 动态高度：解除约束后测得自然高 460；若未解除则只有 57.5（50×F）
      panelHeight: (list) => (list.style.maxHeight === '' ? 460 : 57.5),
      fixture,
    })
    window.innerWidth = VIEWPORT.w
    window.innerHeight = VIEWPORT.h

    applySelectPanelPosition(fixture)

    // 若未解除约束会按 50 设计高判定"下方放得下"而不翻转；正确行为 = 上翻
    expectPx(fixture.panel, 'top', 236)
    expectMaxHeight(fixture.list, 240)
  })

  it('右缘夹持：水平 clamp 保证面板不超出视口（含间距）', () => {
    const fixture = mountFixture()
    mockRects({
      probeWidth: 115,
      // 触发器设计 left 800（视觉 920），右缘视觉 1173 已超出视口 1100
      trigger: { top: 230, bottom: 271.390625, left: 920, width: 253 },
      panelHeight: 460,
      fixture,
    })
    window.innerWidth = VIEWPORT.w
    window.innerHeight = VIEWPORT.h

    applySelectPanelPosition(fixture)

    // left = 视口设计宽 ÷F - 面板设计宽 220 - 间距 4
    const expectedLeft = 1100 / 1.15 - 220 - 4
    expect(parseFloat(fixture.panel.style.left)).toBeCloseTo(expectedLeft, 1)
    // 渲染后面板右缘 ≈ 视口右缘 - 视觉间距 4.6
    const renderedRight = expectedLeft * 1.15 + 253
    expect(renderedRight).toBeLessThanOrEqual(VIEWPORT.w)
    expect(VIEWPORT.w - renderedRight).toBeCloseTo(4.6, 1)
  })

  it('自然高为 0（面板不可测）→ 退回设计高度上限', () => {
    const fixture = mountFixture()
    mockRects({
      probeWidth: 115,
      trigger: { top: 230, bottom: 271.390625, left: 138, width: 253 },
      panelHeight: 0,
      fixture,
    })
    window.innerWidth = VIEWPORT.w
    window.innerHeight = VIEWPORT.h

    applySelectPanelPosition(fixture)

    expectMaxHeight(fixture.list, 240)
  })
})
