import { describe, it, expect, afterEach, vi } from 'vitest'
import {
  getFixedZoomCompensation,
  resetFixedZoomProbe,
} from '../src/ui/zoom-compensation'

/**
 * zoom-compensation 自校准因子单测
 *
 * 背景：Linux 端 html zoom:1.15（标准化 zoom 语义）下，gBCR 读数是视觉坐标、
 * fixed 赋值渲染会再乘 zoom，直接赋值导致面板向右下漂移。因子 F 由探针
 * （set 100px 的 fixed 元素）读回宽度实测。happy-dom 无布局引擎，gBCR 恒 0，
 * 恰好覆盖 F=1 退回路径；zoom 路径以 mock gBCR 模拟。
 */

function mockProbeWidth(width: number) {
  return vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width,
    height: 0,
    top: 0,
    left: 0,
    bottom: 0,
    right: 0,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect)
}

afterEach(() => {
  resetFixedZoomProbe()
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

describe('getFixedZoomCompensation', () => {
  it('无布局引擎（gBCR 恒 0）→ 退回 F=1，与历史无缩放行为一致', () => {
    expect(getFixedZoomCompensation()).toBe(1)
  })

  it('根 zoom 引擎：探针 set 100px 读回 115px → F=1.15（WebKitGTK zoom:1.15 实测值）', () => {
    mockProbeWidth(115)
    expect(getFixedZoomCompensation()).toBe(1.15)
  })

  it('探针被测试清出 body 后自动重挂载，因子实测不受影响', () => {
    mockProbeWidth(115)
    expect(getFixedZoomCompensation()).toBe(1.15)
    document.body.innerHTML = ''
    expect(getFixedZoomCompensation()).toBe(1.15)
  })
})
