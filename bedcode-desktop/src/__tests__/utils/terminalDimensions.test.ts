/**
 * getXtermScaledDimensions 单元测试（Seam A：纯逻辑，DPR 感知行列计算）
 *
 * 覆盖 spec#02：100%/150%/200% 缩放下 cols/rows 精确计算、ceil 行高 / floor
 * 列宽边界（最后一行不裁切、行尾不截断）、退化入参兜底 {1,1}。
 */

import { describe, it, expect } from 'vitest'
import { getXtermScaledDimensions } from '@/utils/terminalDimensions'

const W = 800
const H = 600
const CELL = { cellWidthCss: 7.2, cellHeightCss: 15 }

describe('getXtermScaledDimensions', () => {
  it('100% 缩放（dpr=1）：行列按基础换算', () => {
    expect(
      getXtermScaledDimensions({
        containerWidthCss: W,
        containerHeightCss: H,
        ...CELL,
        devicePixelRatio: 1,
      }),
    ).toEqual({ cols: 111, rows: 40 })
  })

  it('150% 缩放（dpr=1.5）：行高 ceil 上行，行数收敛不溢出', () => {
    // 900(物理高) / ceil(15*1.5=22.5→23) = 39，若不做 DPR ceil 会是 40 → 最后一行放不下
    expect(
      getXtermScaledDimensions({
        containerWidthCss: W,
        containerHeightCss: H,
        ...CELL,
        devicePixelRatio: 1.5,
      }),
    ).toEqual({ cols: 111, rows: 39 })
  })

  it('200% 缩放（dpr=2）：行列精确倍率换算', () => {
    expect(
      getXtermScaledDimensions({
        containerWidthCss: W,
        containerHeightCss: H,
        ...CELL,
        devicePixelRatio: 2,
      }),
    ).toEqual({ cols: 111, rows: 40 })
  })

  it('ceil 行高：非整 cell 行高时最后一行被向上取整（不裁切）', () => {
    // 若直接 floor(600/15.2)=39，实际会放不下第 40 行；ceil(15.2)=16 → floor(600/16)=37
    expect(
      getXtermScaledDimensions({
        containerWidthCss: W,
        containerHeightCss: H,
        cellWidthCss: 7.2,
        cellHeightCss: 15.2,
        devicePixelRatio: 1,
      }),
    ).toEqual({ cols: 111, rows: 37 })
  })

  it('floor 列宽：非整 cell 列宽时行尾不截断', () => {
    // floor(600/8.6)=69（若四舍五入到 70 会超出容器宽度截断末尾列）
    expect(
      getXtermScaledDimensions({
        containerWidthCss: 600,
        containerHeightCss: 300,
        cellWidthCss: 8.6,
        cellHeightCss: 16,
        devicePixelRatio: 1,
      }),
    ).toEqual({ cols: 69, rows: 18 })
  })

  it('退化入参（0/负/非有限）兜底返回 {1,1}', () => {
    const base = { ...CELL, devicePixelRatio: 1 }
    expect(getXtermScaledDimensions({ ...base, containerWidthCss: 0 })).toEqual({ cols: 1, rows: 1 })
    expect(getXtermScaledDimensions({ ...base, containerHeightCss: -10 })).toEqual({
      cols: 1,
      rows: 1,
    })
    expect(getXtermScaledDimensions({ ...base, devicePixelRatio: Number.NaN })).toEqual({
      cols: 1,
      rows: 1,
    })
    expect(getXtermScaledDimensions({ ...base, cellWidthCss: 0 })).toEqual({ cols: 1, rows: 1 })
    expect(getXtermScaledDimensions({ ...base, cellHeightCss: Number.POSITIVE_INFINITY })).toEqual({
      cols: 1,
      rows: 1,
    })
  })

  it('极小容器兜底最小 1 行 1 列（不产生 0 网格）', () => {
    expect(
      getXtermScaledDimensions({
        containerWidthCss: 1,
        containerHeightCss: 1,
        ...CELL,
        devicePixelRatio: 1,
      }),
    ).toEqual({ cols: 1, rows: 1 })
  })
})
