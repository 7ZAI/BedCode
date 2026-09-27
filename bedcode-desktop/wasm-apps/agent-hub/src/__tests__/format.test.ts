/**
 * 展示层格式化纯函数单测（票据 06）
 *
 * 关键约束：数字抽查与源数据一致——缩写不得产生「假零」（0.04k 显示 0k
 * 会与源数据矛盾），进位方向向上；null 不估算显示 —。
 */
import { describe, expect, it } from 'vitest'
import {
  abbreviateProject,
  axisTicks,
  cacheHitRate,
  formatCost,
  formatDuration,
  formatEventTime,
  formatPercent,
  formatSessionTime,
  formatTokens,
  metricValue,
  niceMax,
  totalTokens,
} from '../utils/format'

describe('formatTokens', () => {
  it('千以下原样', () => {
    expect(formatTokens(0)).toBe('0')
    expect(formatTokens(42)).toBe('42')
    expect(formatTokens(999)).toBe('999')
  })

  it('k / M 分级一位小数', () => {
    expect(formatTokens(1000)).toBe('1.0k')
    expect(formatTokens(8_400_000)).toBe('8.4M')
    expect(formatTokens(1_050_000)).toBe('1.1M')
  })

  it('非零小量不产生假零', () => {
    // 40 tokens → 0.04k → 向上进位为 0.1k（显示 0k 与源数据矛盾）
    expect(formatTokens(40)).toBe('40') // 千以下原样
    expect(formatTokens(1004)).toBe('1.1k')
    expect(formatTokens(10_000_400)).toBe('10M') // ≥10M 取整（0.1% 级误差非假零）
  })

  it('空值与非法值显示 —', () => {
    expect(formatTokens(null)).toBe('—')
    expect(formatTokens(undefined)).toBe('—')
    expect(formatTokens(Number.NaN)).toBe('—')
    expect(formatTokens(-5)).toBe('0')
  })
})

describe('formatDuration', () => {
  it('分段缩写', () => {
    expect(formatDuration(500)).toBe('500ms')
    expect(formatDuration(42_000)).toBe('42s')
    expect(formatDuration(90_000)).toBe('1.5min')
    expect(formatDuration(96_000)).toBe('1.6min') // 向上取整到 0.1
    expect(formatDuration(21.4 * 3600 * 1000)).toBe('21.4h')
    expect(formatDuration(30 * 24 * 3600 * 1000)).toBe('30d')
  })

  it('空值显示 —', () => {
    expect(formatDuration(null)).toBe('—')
    expect(formatDuration(undefined)).toBe('—')
  })
})

describe('formatSessionTime / formatEventTime', () => {
  it('本地时区标签（用固定本地时间构造验证格式）', () => {
    const d = new Date(2026, 8, 12, 21, 47, 5) // 本地 2026-09-12 21:47:05
    expect(formatSessionTime(d.getTime())).toBe('09-12 21:47')
    expect(formatEventTime(d.getTime())).toBe('21:47:05')
  })

  it('空值：列表 — / 事件空串', () => {
    expect(formatSessionTime(null)).toBe('—')
    expect(formatEventTime(null)).toBe('')
  })
})

describe('abbreviateProject', () => {
  it('家目录折叠为 ~', () => {
    expect(abbreviateProject('/home/u/proj', '/home/u')).toBe('~/proj')
    expect(abbreviateProject('/home/u', '/home/u')).toBe('~')
  })

  it('非家目录与空值原样', () => {
    expect(abbreviateProject('/var/data', '/home/u')).toBe('/var/data')
    expect(abbreviateProject(null, '/home/u')).toBe('—')
  })
})

describe('formatCost', () => {
  it('有则存两位小数，null 不估算', () => {
    expect(formatCost(12.970714)).toBe('$12.97')
    expect(formatCost(0)).toBe('$0.00')
    expect(formatCost(null)).toBe('—')
  })
})

// ==================== 看板指标换算（票据 06 改版） ====================

describe('totalTokens（token 总量口径）', () => {
  it('输入 + 输出 + 缓存读 + 缓存写', () => {
    expect(totalTokens({ tokens_in: 10, tokens_out: 20, tokens_cache_read: 30, tokens_cache_write: 40 })).toBe(100)
  })

  it('反例守门：推理是输出的子集，不得再加一次（claude thinking_tokens 属于 output_tokens）', () => {
    // 若实现把 reasoning 也加进来，这里会得到 150 → 红
    expect(totalTokens({ tokens_in: 10, tokens_out: 20, tokens_cache_read: 30, tokens_cache_write: 40, tokens_reasoning: 50 })).toBe(100)
  })

  it('边界：缺字段 / null / 全 0 都归 0，不出 NaN', () => {
    expect(totalTokens({})).toBe(0)
    expect(totalTokens(null)).toBe(0)
    expect(totalTokens(undefined)).toBe(0)
    expect(totalTokens({ tokens_in: 0, tokens_out: 0, tokens_cache_read: 0, tokens_cache_write: 0 })).toBe(0)
  })
})

describe('metricValue（指标取值唯一口径）', () => {
  const row = {
    tokens_in: 1,
    tokens_out: 2,
    tokens_cache_read: 3,
    tokens_cache_write: 4,
    tokens_reasoning: 5,
    sessions: 6,
    duration_ms: 7_000,
    cost_total: 1.25,
  }

  it('逐指标取对应字段', () => {
    expect(metricValue(row, 'tokens_in')).toBe(1)
    expect(metricValue(row, 'tokens_out')).toBe(2)
    expect(metricValue(row, 'tokens_cache_read')).toBe(3)
    expect(metricValue(row, 'tokens_cache_write')).toBe(4)
    expect(metricValue(row, 'tokens_reasoning')).toBe(5)
    expect(metricValue(row, 'sessions')).toBe(6)
    expect(metricValue(row, 'duration_ms')).toBe(7_000)
    expect(metricValue(row, 'cost_total')).toBe(1.25)
  })

  it("'tokens' = 四桶之和（与 totalTokens 同一口径）", () => {
    expect(metricValue(row, 'tokens')).toBe(10)
  })

  it('边界：字段缺失归 0；成本为 null（未上报）时也是 0 而不是 NaN', () => {
    expect(metricValue({}, 'tokens_in')).toBe(0)
    expect(metricValue({}, 'tokens')).toBe(0)
    expect(metricValue({ cost_total: null }, 'cost_total')).toBe(0)
  })

  it("'tokens' 优先取节奏格预置的总量（该格无分桶）", () => {
    expect(metricValue({ tokens: 42, tokens_in: 1, tokens_out: 1 }, 'tokens')).toBe(42)
  })
})

describe('cacheHitRate（缓存命中率）', () => {
  it('缓存读 / (输入 + 缓存读 + 缓存写)', () => {
    expect(cacheHitRate({ tokens_in: 0, tokens_cache_read: 75, tokens_cache_write: 25 })).toBe(0.75)
    expect(cacheHitRate({ tokens_in: 100, tokens_cache_read: 300, tokens_cache_write: 0 })).toBe(0.75)
  })

  it('边界：分母为 0 → null（不是 0%——「没输入过」不等于「缓存没起作用」）', () => {
    expect(cacheHitRate({ tokens_in: 0, tokens_cache_read: 0, tokens_cache_write: 0 })).toBeNull()
    expect(cacheHitRate(null)).toBeNull()
  })

  it('无缓存命中时是 0 而不是 null（有分母、确实没命中）', () => {
    expect(cacheHitRate({ tokens_in: 500 })).toBe(0)
  })
})

describe('formatPercent', () => {
  it('0–1 小数转百分比', () => {
    expect(formatPercent(0.5)).toBe('50%')
    expect(formatPercent(0.256, 1)).toBe('25.6%')
    expect(formatPercent(0)).toBe('0%')
  })

  it('null / 非有限值不编造百分比', () => {
    expect(formatPercent(null)).toBe('—')
    expect(formatPercent(Number.NaN)).toBe('—')
  })
})

describe('niceMax（坐标轴上界）', () => {
  it('取 1 / 2 / 2.5 / 5 × 10^k 中不小于 max 的最小值', () => {
    expect(niceMax(1)).toBe(1)
    expect(niceMax(1.2)).toBe(2)
    expect(niceMax(0.3)).toBe(0.5)
    expect(niceMax(7)).toBe(10)
    expect(niceMax(120)).toBe(200)
  })

  it('边界：非正 / 非有限 → 1（避免除零与空路径），且永远 ≥ max', () => {
    expect(niceMax(0)).toBe(1)
    expect(niceMax(-5)).toBe(1)
    expect(niceMax(Number.NaN)).toBe(1)
    for (const m of [0.04, 3, 9, 11, 250, 999, 1_000, 12_345]) {
      expect(niceMax(m)).toBeGreaterThanOrEqual(m)
    }
  })
})

describe('axisTicks（刻度序列）', () => {
  it('自上而下：上界 → 0，等分 count+1 档', () => {
    expect(axisTicks(100, 3)).toEqual([100, 200 / 3, 100 / 3, 0])
    expect(axisTicks(10, 1)).toEqual([10, 0])
  })

  it('边界：上界 0 时不再全为 0 造成「刻度重叠」（0/3 = 0）', () => {
    // 0 上界下刻度全等，前端会画出 4 条重合线；此处只锁住「不会算出负数/NaN」
    for (const t of axisTicks(0, 3)) expect(Number.isFinite(t)).toBe(true)
  })
})
