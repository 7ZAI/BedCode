/**
 * 终端网格 DPR 感知计算测试（对齐桌面端 terminalDimensions 语义）
 *
 * 关键口径：行尾右缘**不预留**（TERMINAL_RIGHT_RESERVE_PX = 0）——多留一格就会在
 * 容器右缘留下一条无单元格绘制的差额带，TUI（opencode）自带底色铺满列宽时那条带
 * 就是「右侧竖直黑带」。故 cols 必须等于 floor(容器宽 / 格宽)。
 */
import { describe, it, expect } from 'vitest'
import { getXtermScaledDimensions } from '@/utils/terminalDimensions'
import { TERMINAL_RIGHT_RESERVE_PX } from '@/utils/terminalMetrics'

const base = {
  containerWidthCss: 1000,
  containerHeightCss: 500,
  cellWidthCss: 10,
  cellHeightCss: 20,
}

describe('getXtermScaledDimensions (DPR 感知)', () => {
  it('行尾不预留：cols = floor(容器宽 / 格宽)，不因右缘留白少一列', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1 })
    // 无预留：可用宽=1000，格宽=10 → cols=100；可用高=500，行高=20 → rows=25
    expect(r).toEqual({ cols: 100, rows: 25 })
  })

  it('DPR=2：CSS像素×DPR 换算物理像素', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 2 })
    // 物理可用宽=1000*2=2000，格宽=10*2=20 → cols=100
    // 物理可用高=500*2=1000，行高=ceil(20*2)=40 → rows=25
    expect(r).toEqual({ cols: 100, rows: 25 })
  })

  it('DPR=1.5（150% 缩放）', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1.5 })
    // 物理可用宽=1500，格宽=15 → cols=100；物理可用高=750，行高=30 → rows=25
    expect(r).toEqual({ cols: 100, rows: 25 })
  })

  it('手机实测口径：容器 407 / 格宽 7.5 → 54 列（余 2px 发丝线，非 17px 黑带）', () => {
    const r = getXtermScaledDimensions({
      containerWidthCss: 407,
      containerHeightCss: 700,
      cellWidthCss: 7.5,
      cellHeightCss: 18,
      devicePixelRatio: 3,
    })
    expect(r.cols).toBe(54)
    // 差额 = 容器宽 - cols×格宽 = 407 - 405 = 2px < 1 格（旧口径会多扣 6+7.5=13.5px）
    expect(407 - r.cols * 7.5).toBeLessThan(7.5)
  })

  it('cell 高×DPR 为小数时不 ceil（原始值），最后一行仍放得下', () => {
    // cell 高 15px，DPR=1.5 → char高=22.5（不 ceil）：rows=floor(750/22.5)=33
    // 33×22.5=742.5 ≤ 750 物理可用高，最后一行完整不裁；ceil 反会高估每行
    // 成本（23px）少算 1 行（32），贴底对齐时顶部露出多余空带
    const r = getXtermScaledDimensions({ ...base, cellHeightCss: 15, devicePixelRatio: 1.5 })
    expect(r.rows).toBe(33)
  })

  it('marginCols/marginRows 额外扣除格数（调用方显式传入才扣）', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1, marginCols: 2, marginRows: 1 })
    // 可用宽再减 2*10=20 → 980；格宽=10 → cols=98
    // 可用高再减 1*20 → 480；行高=20 → rows=24
    expect(r).toEqual({ cols: 98, rows: 24 })
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
    // 兜底精确语义：DPR=0 与 DPR=1 计算结果完全一致（变异：把 <=0 改为 <0 → 本断言失败）
    expect(r).toEqual(getXtermScaledDimensions({ ...base, devicePixelRatio: 1 }))
    // 负数 DPR 同样按 1 兜底
    expect(getXtermScaledDimensions({ ...base, devicePixelRatio: -2 })).toEqual(
      getXtermScaledDimensions({ ...base, devicePixelRatio: 1 }),
    )
  })

  it('显式 marginCols 吃满可用宽 → cols 钳制为 1（行数不受影响）', () => {
    // 容器 20px、格宽 10px、marginCols=2 → 可用宽 0 → floor 得 0 → 钳制为 1
    const r = getXtermScaledDimensions({
      ...base,
      devicePixelRatio: 1,
      containerWidthCss: 20,
      marginCols: 2,
    })
    expect(r.cols).toBe(1)
    expect(r.rows).toBe(25)
  })

  it('恒为非零合法维度（极小容器也拿 1 行 1 列）', () => {
    const r = getXtermScaledDimensions({ ...base, devicePixelRatio: 1, containerWidthCss: 5, containerHeightCss: 5 })
    expect(r.cols).toBeGreaterThanOrEqual(1)
    expect(r.rows).toBeGreaterThanOrEqual(1)
  })

  it('TERMINAL_RIGHT_RESERVE_PX 锁 0：右缘不预留（回归即右侧竖直黑带）', () => {
    expect(TERMINAL_RIGHT_RESERVE_PX).toBe(0)
    // 常量一旦被改回 >0，本断言直接失败——它是「不留黑带」的契约锁
    const withReserve = getXtermScaledDimensions({
      ...base,
      devicePixelRatio: 1,
      marginCols: 0,
    })
    expect(withReserve.cols).toBe(Math.floor(base.containerWidthCss / base.cellWidthCss))
  })
})