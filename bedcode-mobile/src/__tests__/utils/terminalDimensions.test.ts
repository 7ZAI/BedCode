/**
 * 终端网格 DPR 感知计算测试（对齐桌面端 terminalDimensions 语义 + 移动端
 * 对齐 FitAddon 裸 fit 的滚动条扣除 / margin 参数）
 */
import { describe, it, expect } from 'vitest'
import { getXtermScaledDimensions } from '@/utils/terminalDimensions'

const base = {
  containerWidthCss: 1000,
  containerHeightCss: 500,
  cellWidthCss: 10,
  cellHeightCss: 20,
}

describe('getXtermScaledDimensions (DPR 感知)', () => {
  it('DPR=1：滚动条 14px 扣除，列宽 floor / 行高线性', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1 })
    // 可用宽=1000-14=986，char宽=10 → cols=98；可用高=500，char高=20 → rows=25
    expect(r).toEqual({ cols: 98, rows: 25 })
  })

  it('DPR=2：CSS像素×DPR 换算物理像素', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 2 })
    // 物理可用宽=1000*2-14*2=1972，char宽=10*2=20 → cols=98
    // 物理可用高=500*2=1000，char高=ceil(20*2)=40 → rows=25
    expect(r).toEqual({ cols: 98, rows: 25 })
  })

  it('DPR=1.5（150% 缩放）', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1.5 })
    // 物理可用宽=1000*1.5-14*1.5=1479，char宽=10*1.5=15 → cols=98
    // 物理可用高=500*1.5=750，char高=ceil(20*1.5)=30 → rows=25
    expect(r).toEqual({ cols: 98, rows: 25 })
  })

  it('cell 高×DPR 为小数时 ceil：保证最后一行放得下', () => {
    // cell 高 15px，DPR=1.5 → char高=ceil(22.5)=23（非 floor）
    const r = getXtermScaledDimensions({ ...base, cellHeightCss: 15, devicePixelRatio: 1.5 })
    // 可用高=750，char高=ceil(22.5)=23 → rows=floor(750/23)=32
    expect(r.rows).toBe(32)
  })

  it('marginCols/marginRows 额外扣除格数', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1, marginCols: 2, marginRows: 1 })
    // 可用宽再减 2*10=20 → 966；char=10 → cols=96
    // 可用高再减 1*20 → 480；char=20 → rows=24
    expect(r).toEqual({ cols: 96, rows: 24 })
  })

  it('退化入参（≤0 / NaN）返回 {1,1} 兜底', () => {
    expect(getXtermScaledDimensions({ ...base, devicePixelRatio: 1, containerWidthCss: 0 })).toEqual({ cols: 1, rows: 1 })
    expect(getXtermScaledDimensions({ ...base, devicePixelRatio: 1, cellWidthCss: 0 })).toEqual({ cols: 1, rows: 1 })
    expect(getXtermScaledDimensions({ ...base, devicePixelRatio: 1, containerHeightCss: NaN })).toEqual({ cols: 1, rows: 1 })
  })

  it('非法 DPR（≤0）按 1 兜底，不产生 NaN', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 0 })
    expect(r.cols).toBeGreaterThan(0)
    expect(r.rows).toBeGreaterThan(0)
    expect(Number.isFinite(r.cols)).toBe(true)
  })

  it('恒为非零合法维度（极小容器也拿 1 行 1 列）', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1, containerWidthCss: 5, containerHeightCss: 5 })
    expect(r.cols).toBeGreaterThanOrEqual(1)
    expect(r.rows).toBeGreaterThanOrEqual(1)
  })
})