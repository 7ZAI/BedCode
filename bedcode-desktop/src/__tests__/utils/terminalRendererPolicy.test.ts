/**
 * terminalRendererPolicy 单元测试（Seam A：纯逻辑）
 *
 * 覆盖 decideRenderer 契约：isLinux × hasBackgroundImage × linuxUseDomRenderer
 * × route('A'|'B') 全组合（16 组）断言 { useDom, useWebgl, allowTransparency }。
 * 关键不变量：route B 下背景图开启时绝对不用 WebGL（无论平台）；
 * 无背景图 + 非 Linux + route B 必须 WebGL 且不透明（默认零残影场景不被回归）。
 * 覆盖 decideAtlasRefreshFrames 契约：帧预算逐帧递减、到 0 停止、
 * 传 0 / 负数防御性停止。
 */

import { describe, it, expect } from 'vitest'
import {
  decideRenderer,
  decideAtlasRefreshFrames,
  ATLAS_PREHEAT_FRAME_BUDGET,
} from '@/utils/terminalRendererPolicy'

/**
 * 期望表：每个元素为 [isLinux, hasBackgroundImage, linuxUseDomRenderer, useDom, useWebgl, allowTransparency]。
 * 由 decideRenderer 语义推导（route B 表在 A 表基础上把 hasBackgroundImage=true 的 WebGL 行改为 DOM）。
 */
// route 'B'：hasBackgroundImage=true 或 (isLinux && linuxUseDomRenderer) → useDom=true
const routeBExpectations: Array<[boolean, boolean, boolean, boolean, boolean, boolean]> = [
  // 无背景图 + 非 Linux：无论 linuxUseDomRenderer 都走 WebGL、不透明（默认场景不受路线切换影响）
  [false, false, false, false, true, false],
  [false, false, true, false, true, false],
  // 背景图 + 非 Linux：强制 DOM，透明正确（useWebgl 无真值）
  [false, true, false, true, false, true],
  [false, true, true, true, false, true],
  // 无背景图 + Linux：只有 linuxUseDomRenderer 成立才走 DOM（既有 LINUX_USE_DOM_RENDERER 事实）
  [true, false, false, false, true, false],
  [true, false, true, true, false, false],
  // 背景图 + Linux：无论 linuxUseDomRenderer 都强制 DOM
  [true, true, false, true, false, true],
  [true, true, true, true, false, true],
]

// route 'A'：useDom = (isLinux && linuxUseDomRenderer)，其余 WebGL；透明恒随背景图
const routeAExpectations: Array<[boolean, boolean, boolean, boolean, boolean, boolean]> = [
  [false, false, false, false, true, false],
  [false, false, true, false, true, false],
  // 背景图 + 非 Linux：A 路线保留 WebGL + 透明（残影问题交给 D-1 重建修复）
  [false, true, false, false, true, true],
  [false, true, true, false, true, true],
  [true, false, false, false, true, false],
  [true, false, true, true, false, false],
  [true, true, false, false, true, true],
  [true, true, true, true, false, true],
]

describe('decideRenderer', () => {
  it.each(routeBExpectations.map((row) => ({ row })))(
    'route B %j',
    ({ row }) => {
      const [isLinux, hasBackgroundImage, linuxUseDomRenderer, useDom, useWebgl, allowTransparency] = row
      expect(
        decideRenderer({ isLinux, hasBackgroundImage, linuxUseDomRenderer, route: 'B' }),
      ).toEqual({ useDom, useWebgl, allowTransparency })
    },
  )

  it.each(routeAExpectations.map((row) => ({ row })))(
    'route A %j',
    ({ row }) => {
      const [isLinux, hasBackgroundImage, linuxUseDomRenderer, useDom, useWebgl, allowTransparency] = row
      expect(
        decideRenderer({ isLinux, hasBackgroundImage, linuxUseDomRenderer, route: 'A' }),
      ).toEqual({ useDom, useWebgl, allowTransparency })
    },
  )

  it('route 缺省时按 B 处理（spec D-2 建议路线，安全默认）', () => {
    // 背景图 + 非 Linux：缺省 route 必须落到 B，即强制 DOM
    expect(decideRenderer({ isLinux: false, hasBackgroundImage: true, linuxUseDomRenderer: false })).toEqual({
      useDom: true,
      useWebgl: false,
      allowTransparency: true,
    })
    // 无背景图 + 非 Linux：缺省 route 落到 B 的默认路径（WebGL、不透明）
    expect(decideRenderer({ isLinux: false, hasBackgroundImage: false, linuxUseDomRenderer: false })).toEqual({
      useDom: false,
      useWebgl: true,
      allowTransparency: false,
    })
  })

  it('关键不变量：route B + 背景图 → 无论 isLinux 都 useWebgl=false（透明通道只留给 DOM）', () => {
    for (const isLinux of [true, false]) {
      for (const linuxUseDomRenderer of [true, false]) {
        const d = decideRenderer({ isLinux, hasBackgroundImage: true, linuxUseDomRenderer, route: 'B' })
        expect(d.useWebgl).toBe(false)
        expect(d.useDom).toBe(true)
        expect(d.allowTransparency).toBe(true)
      }
    }
  })

  it('关键不变量：无背景图 + 非 Linux + B → useWebgl=true、allowTransparency=false（默认场景零残影不被回归）', () => {
    for (const linuxUseDomRenderer of [true, false]) {
      const d = decideRenderer({ isLinux: false, hasBackgroundImage: false, linuxUseDomRenderer, route: 'B' })
      expect(d).toEqual({ useDom: false, useWebgl: true, allowTransparency: false })
    }
  })

  it('输出约束：useDom 与 useWebgl 恒为互斥（渲染器二选一，不存在双渲染器状态）', () => {
    for (const isLinux of [true, false]) {
      for (const hasBackgroundImage of [true, false]) {
        for (const linuxUseDomRenderer of [true, false]) {
          for (const route of ['A', 'B'] as const) {
            const d = decideRenderer({ isLinux, hasBackgroundImage, linuxUseDomRenderer, route })
            expect(d.useDom).not.toBe(d.useWebgl)
          }
        }
      }
    }
  })
})

describe('decideAtlasRefreshFrames', () => {
  it('帧预算逐帧递减：8 → 7（初始预算常量为 8）', () => {
    expect(ATLAS_PREHEAT_FRAME_BUDGET).toBe(8)
    expect(decideAtlasRefreshFrames(ATLAS_PREHEAT_FRAME_BUDGET)).toBe(7)
  })

  it('模拟完整迭代：从初始预算一路递减到 0 即停止', () => {
    let frames = ATLAS_PREHEAT_FRAME_BUDGET
    const seen: number[] = []
    while (frames > 0) {
      frames = decideAtlasRefreshFrames(frames)
      seen.push(frames)
    }
    // 8 帧预算共产生 7 次后续帧数值，最后一次为 0
    expect(seen).toEqual([7, 6, 5, 4, 3, 2, 1, 0])
  })

  it('预算耗尽（传 0）返回 0：迭代循环条件收敛，不再下发刷新', () => {
    expect(decideAtlasRefreshFrames(0)).toBe(0)
    expect(decideAtlasRefreshFrames(decideAtlasRefreshFrames(1))).toBe(0)
  })

  it('负数输入防御性返回 0：预算被外部污染（销毁/切换残留回调）时立刻停止而非继续递减', () => {
    expect(decideAtlasRefreshFrames(-1)).toBe(0)
    expect(decideAtlasRefreshFrames(-100)).toBe(0)
  })
})