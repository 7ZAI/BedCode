/**
 * 实机分布夹具驱动的图表自洽检查
 *
 * 下面的数据是**本机 agent-hub 插件库 160 条真实会话的聚合结果**（2026-08-30
 * ~ 09-20，claude 50 + pi 110，8 个项目，9 个活跃日，42 个非空时段格），
 * 于 2026-09-27 从 `plugins/com.bedcode.agent-hub/plugin.db` 导出。
 *
 * 为什么要留这份夹具而不是用合成数据：图表的真实风险全在**偏态**上——
 * 单日 token 最高 72M、最低 2 万（相差 3 万倍）、单时段最高 19.3M。合成
 * 均匀数据会让线性分档、单点坐标、极端 y 值这些问题一个都测不出来。
 *
 * 断言全部是**可判定的数值关系**（和 > 100%、y 在 viewBox 内、条宽单调、
 * 百分比求和 ≈100%），不是「看起来对」。
 */
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import StatsTrend from '../components/StatsTrend.vue'
import StatsHeatmap from '../components/StatsHeatmap.vue'
import StatsDonut from '../components/StatsDonut.vue'
import StatsBars from '../components/StatsBars.vue'
import { totalTokens, metricValue, cacheHitRate } from '../utils/format'
import type { CliStatRow, DayStatRow, HourCell, ProjectStatRow } from '../types'

const TOTAL = {
  sessions: 160,
  tokens_in: 21018177,
  tokens_out: 1597676,
  tokens_cache_read: 197265900,
  tokens_cache_write: 0,
  tokens_reasoning: 613538,
  duration_ms: 123316239,
  cost_total: 9.168800621599999,
  active_days: 9,
  projects: 8,
}
const BY_DAY: DayStatRow[] = [
  { day: '2026-08-30', sessions: 2, tokens_in: 20505, tokens_out: 225, tokens_cache_read: 128, tokens_cache_write: 0, tokens_reasoning: 0, duration_ms: 709411, cost_total: 0 },
  { day: '2026-09-04', sessions: 86, tokens_in: 6875726, tokens_out: 549704, tokens_cache_read: 72105546, tokens_cache_write: 0, tokens_reasoning: 175946, duration_ms: 58636081, cost_total: 8.8971589688 },
  { day: '2026-09-05', sessions: 43, tokens_in: 4261820, tokens_out: 391273, tokens_cache_read: 61159714, tokens_cache_write: 0, tokens_reasoning: 72610, duration_ms: 19371296, cost_total: 0 },
  { day: '2026-09-06', sessions: 7, tokens_in: 883049, tokens_out: 120170, tokens_cache_read: 9653760, tokens_cache_write: 0, tokens_reasoning: 69803, duration_ms: 13457846, cost_total: 0.270816 },
  { day: '2026-09-07', sessions: 3, tokens_in: 407854, tokens_out: 43132, tokens_cache_read: 7889152, tokens_cache_write: 0, tokens_reasoning: 10226, duration_ms: 4559448, cost_total: 0.0008256527999999999 },
  { day: '2026-09-08', sessions: 13, tokens_in: 6503812, tokens_out: 374104, tokens_cache_read: 39687424, tokens_cache_write: 0, tokens_reasoning: 212519, duration_ms: 20528938, cost_total: 0 },
  { day: '2026-09-10', sessions: 3, tokens_in: 1451275, tokens_out: 82109, tokens_cache_read: 4893696, tokens_cache_write: 0, tokens_reasoning: 52455, duration_ms: 2988346, cost_total: 0 },
  { day: '2026-09-14', sessions: 1, tokens_in: 510828, tokens_out: 27758, tokens_cache_read: 1020928, tokens_cache_write: 0, tokens_reasoning: 15273, duration_ms: 1637134, cost_total: 0 },
  { day: '2026-09-20', sessions: 2, tokens_in: 103308, tokens_out: 9201, tokens_cache_read: 855552, tokens_cache_write: 0, tokens_reasoning: 4706, duration_ms: 1427739, cost_total: 0 },
]
const BY_CLI: CliStatRow[] = [
  { adapter: 'pi', sessions: 110, duration_ms: 110311884, tokens_in: 20276936, tokens_out: 1509013, tokens_cache_read: 184633496, tokens_cache_write: 0, tokens_reasoning: 613538, cost_total: 0.0013411216, last_at: 1789908329767 },
  { adapter: 'claude', sessions: 50, duration_ms: 13004355, tokens_in: 741241, tokens_out: 88663, tokens_cache_read: 12632404, tokens_cache_write: 0, tokens_reasoning: 0, cost_total: 9.1674595, last_at: 1788650944097 },
]
const BY_PROJECT: ProjectStatRow[] = [
  { project: '/home/binblink/project/tauriProject/BedCode', sessions: 106, duration_ms: 59773289, tokens_in: 8615657, tokens_out: 767725, tokens_cache_read: 119110023, tokens_cache_write: 0, tokens_reasoning: 166652, cost_total: 9.1674595, last_at: 1788650944097 },
  { project: '/home/binblink', sessions: 42, duration_ms: 61349578, tokens_in: 12213361, tokens_out: 821604, tokens_cache_read: 77364649, tokens_cache_write: 0, tokens_reasoning: 446705, cost_total: 0.0013411216, last_at: 1789907882768 },
  { project: '/home/binblink/project/tauriProject/BedCode/bedcode-mobile', sessions: 1, duration_ms: 778234, tokens_in: 120379, tokens_out: 5971, tokens_cache_read: 762669, tokens_cache_write: 0, tokens_reasoning: 0, cost_total: null, last_at: 1788523548218 },
  { project: '/tmp', sessions: 3, duration_ms: 33833, tokens_in: 15717, tokens_out: 1207, tokens_cache_read: 23861, tokens_cache_write: 0, tokens_reasoning: 0, cost_total: 0, last_at: 1788454734795 },
  { project: '/home/binblink/Desktop/project/tauriProject/BedCode', sessions: 3, duration_ms: 170419, tokens_in: 24641, tokens_out: 297, tokens_cache_read: 4570, tokens_cache_write: 0, tokens_reasoning: 138, cost_total: 0, last_at: 1788629110211 },
  { project: 'E:\tauriProject\BedCode', sessions: 2, duration_ms: 709411, tokens_in: 20505, tokens_out: 225, tokens_cache_read: 128, tokens_cache_write: 0, tokens_reasoning: 0, cost_total: 0, last_at: 1788092524258 },
  { project: '/tmp/worktrees/gittest/pi-worktree-d926f11e-8113-49bb-b244-a3265c10e803-0', sessions: 1, duration_ms: 17477, tokens_in: 7917, tokens_out: 647, tokens_cache_read: 0, tokens_cache_write: 0, tokens_reasoning: 43, cost_total: 0, last_at: 1788802552814 },
  { project: '', sessions: 1, duration_ms: 483886, tokens_in: 0, tokens_out: 0, tokens_cache_read: 0, tokens_cache_write: 0, tokens_reasoning: 0, cost_total: null, last_at: 1789908329767 },
  { project: '/home/binblink/project/tauriProject/BedCode/bedcode-mobile/plugins/ai-chatbox', sessions: 1, duration_ms: 112, tokens_in: 0, tokens_out: 0, tokens_cache_read: 0, tokens_cache_write: 0, tokens_reasoning: 0, cost_total: 0, last_at: 1788612058255 },
]
const BY_HOUR: HourCell[] = [
  { dow: 0, hour: 1, sessions: 1, tokens: 16484 },
  { dow: 0, hour: 6, sessions: 1, tokens: 8574088 },
  { dow: 0, hour: 7, sessions: 1, tokens: 388350 },
  { dow: 0, hour: 8, sessions: 1, tokens: 0 },
  { dow: 0, hour: 19, sessions: 2, tokens: 1427170 },
  { dow: 0, hour: 20, sessions: 4, tokens: 1110523 },
  { dow: 0, hour: 23, sessions: 1, tokens: 129283 },
  { dow: 1, hour: 2, sessions: 1, tokens: 7676936 },
  { dow: 1, hour: 20, sessions: 2, tokens: 2035379 },
  { dow: 1, hour: 21, sessions: 1, tokens: 187337 },
  { dow: 2, hour: 0, sessions: 2, tokens: 10023529 },
  { dow: 2, hour: 1, sessions: 5, tokens: 5684935 },
  { dow: 2, hour: 2, sessions: 2, tokens: 12772720 },
  { dow: 2, hour: 3, sessions: 1, tokens: 10700901 },
  { dow: 2, hour: 4, sessions: 1, tokens: 133185 },
  { dow: 2, hour: 5, sessions: 1, tokens: 199790 },
  { dow: 2, hour: 6, sessions: 1, tokens: 7050280 },
  { dow: 4, hour: 7, sessions: 1, tokens: 103101 },
  { dow: 4, hour: 8, sessions: 1, tokens: 2917969 },
  { dow: 4, hour: 9, sessions: 1, tokens: 3406010 },
  { dow: 5, hour: 0, sessions: 9, tokens: 9925095 },
  { dow: 5, hour: 1, sessions: 7, tokens: 3282138 },
  { dow: 5, hour: 2, sessions: 1, tokens: 2079475 },
  { dow: 5, hour: 3, sessions: 2, tokens: 5996949 },
  { dow: 5, hour: 4, sessions: 3, tokens: 11756961 },
  { dow: 5, hour: 6, sessions: 2, tokens: 14956664 },
  { dow: 5, hour: 7, sessions: 33, tokens: 1108731 },
  { dow: 5, hour: 8, sessions: 14, tokens: 8439290 },
  { dow: 5, hour: 9, sessions: 3, tokens: 791869 },
  { dow: 5, hour: 19, sessions: 3, tokens: 3138396 },
  { dow: 5, hour: 20, sessions: 2, tokens: 5390320 },
  { dow: 5, hour: 21, sessions: 2, tokens: 4874459 },
  { dow: 5, hour: 22, sessions: 3, tokens: 5617788 },
  { dow: 5, hour: 23, sessions: 2, tokens: 2172841 },
  { dow: 6, hour: 0, sessions: 2, tokens: 3463686 },
  { dow: 6, hour: 1, sessions: 2, tokens: 19347113 },
  { dow: 6, hour: 2, sessions: 2, tokens: 13041433 },
  { dow: 6, hour: 4, sessions: 1, tokens: 536279 },
  { dow: 6, hour: 5, sessions: 15, tokens: 14246904 },
  { dow: 6, hour: 6, sessions: 19, tokens: 13627605 },
  { dow: 6, hour: 20, sessions: 1, tokens: 0 },
  { dow: 6, hour: 23, sessions: 1, tokens: 1549787 },
]


/** i18n 桩：直返 key 并渲染插值参数（便于断言峰值时段文案） */
const ctx = {
  i18n: {
    t: (k: string, p?: Record<string, unknown>) =>
      p && Object.keys(p).length ? `${k}(${JSON.stringify(p)})` : k,
    getI18n: () => undefined,
  },
  commands: { execute: async () => null },
  events: { on: () => ({ dispose: () => {} }) },
} as unknown as PluginContext

function mountIt(component: unknown, props: Record<string, unknown>) {
  return mount(component as never, {
    props,
    global: { provide: { pluginContext: ctx } },
  } as never)
}

describe('实机偏态数据驱动图表（160 会话 / 8 项目 / 2 CLI / 42 时段）', () => {
  it('夹具本身符合实机特征（否则下面的断言失去意义）', () => {
    expect(TOTAL.sessions).toBe(160)
    expect(BY_DAY).toHaveLength(9)
    expect(BY_HOUR).toHaveLength(42)
    // 偏态：单日最大 / 最小相差 3 万倍量级
    const dayTotals = BY_DAY.map((r) => totalTokens(r))
    expect(Math.max(...dayTotals) / Math.min(...dayTotals.filter((v) => v > 0))).toBeGreaterThan(1000)
  })

  it('汇总口径：token 总量 = 四桶和，且 ≥ 推理（推理是输出的子集）', () => {
    const t = totalTokens(TOTAL)
    expect(t).toBe(
      TOTAL.tokens_in + TOTAL.tokens_out + TOTAL.tokens_cache_read + TOTAL.tokens_cache_write,
    )
    expect(t).toBeGreaterThan(TOTAL.tokens_reasoning)
    // 缓存是本机用量的重头戏，命中率应显著大于 0
    expect(cacheHitRate(TOTAL)).toBeGreaterThan(0.5)
  })

  it('趋势图：单指标与堆叠路径的图元全为有限数', () => {
    const single = mountIt(StatsTrend, { rows: BY_DAY, metric: 'tokens', mode: 'total' })
    expect(single.find('.ah-trend-line').attributes('d')).not.toMatch(/NaN|Infinity/)
    expect(single.find('.ah-trend-area').attributes('d')).not.toMatch(/NaN|Infinity/)
    single.unmount()

    const stack = mountIt(StatsTrend, { rows: BY_DAY, metric: 'tokens', mode: 'stack' })
    expect(stack.findAll('.ah-trend-band')).toHaveLength(4)
    for (const b of stack.findAll('.ah-trend-band')) {
      expect(b.attributes('d')).not.toMatch(/NaN|Infinity/)
    }
    stack.unmount()
  })

  it('趋势图：图元 y 坐标落在 viewBox 内（偏态下不能画到画布外）', () => {
    const w = mountIt(StatsTrend, { rows: BY_DAY, metric: 'tokens', mode: 'stack' })
    for (const b of w.findAll('.ah-trend-band')) {
      for (const m of (b.attributes('d') ?? '').matchAll(/,(-?[\d.]+)/g)) {
        const yv = Number(m[1])
        expect(yv).toBeGreaterThanOrEqual(-0.01)
        expect(yv).toBeLessThanOrEqual(180.01)
      }
    }
    w.unmount()
  })

  it('趋势图：x 标签不超过 6 个，首尾落在真实首末日', () => {
    const w = mountIt(StatsTrend, { rows: BY_DAY, metric: 'tokens', mode: 'total' })
    const ticks = w.findAll('.ah-trend-xtick')
    expect(ticks.length).toBeLessThanOrEqual(6)
    expect(ticks[0].text()).toBe(BY_DAY[0].day.slice(5))
    expect(ticks[ticks.length - 1].text()).toBe(BY_DAY[BY_DAY.length - 1].day.slice(5))
    w.unmount()
  })

  it('节奏热力图：168 格、峰值与库中最大格一致、四档全部用到', () => {
    const w = mountIt(StatsHeatmap, { cells: BY_HOUR })
    expect(w.findAll('.ah-heat-cell')).toHaveLength(168)
    const best = BY_HOUR.reduce((a, b) => (b.tokens > a.tokens ? b : a))
    const peak = w.get('.ah-heat-peak').text()
    // 渲染侧小时是补零字符串（"01"），星期走 dow 键
    expect(peak).toContain(String(best.hour).padStart(2, '0'))
    expect(peak).toContain(`hub.st.heat.dow.${best.dow}`)
    const levels = new Set(
      w.findAll('.ah-heat-cell').map((c) => (c.classes().find((k) => k.startsWith('lv')) ?? '').slice(2)),
    )
    // 名次分档在真实偏态下仍能用满 4 档（线性分档会退化成 2 档）
    expect([...levels].sort()).toEqual(['0', '1', '2', '3', '4'])
    w.unmount()
  })

  it('占比环：百分比之和 ≈ 100%，各段弧长之和 < 整圈（必须留缝）', () => {
    const slices = BY_CLI.map((r, i) => ({
      key: r.adapter,
      label: r.adapter,
      value: totalTokens(r),
      colorIndex: i,
    }))
    const w = mountIt(StatsDonut, { slices, centerValue: 'x', centerLabel: 'token' })
    const sum = w.findAll('.ah-donut-legend-pct').map((e) => parseFloat(e.text())).reduce((a, b) => a + b, 0)
    expect(sum).toBeGreaterThan(98)
    expect(sum).toBeLessThan(101)
    const circle = 2 * Math.PI * 62
    const dashes = w
      .findAll('.ah-donut-arc')
      .map((a) => Number((a.attributes('stroke-dasharray') ?? '').split(' ')[0]))
    expect(dashes.reduce((a, b) => a + b, 0)).toBeLessThan(circle)
    w.unmount()
  })

  it('项目排行：按 tokens 降序、条宽单调不增、首行满格且不越界', () => {
    const rows = BY_PROJECT.slice(0, 8).map((r, i) => ({
      key: `p${i}`,
      label: r.project || '未知',
      value: totalTokens(r),
      valueText: String(totalTokens(r)),
    }))
    const w = mountIt(StatsBars, { rows, emptyText: '—' })
    // style 形如 `width: 71.99%;` —— 取首个百分比数字
    const widths = w
      .findAll('.ah-bars-fill')
      .map((f) => parseFloat((f.attributes('style') ?? '').match(/([\d.]+)%/)?.[1] ?? '0'))
    expect(widths[0]).toBeCloseTo(100, 1)
    for (let i = 1; i < widths.length; i++) expect(widths[i]).toBeLessThanOrEqual(widths[i - 1])
    expect(widths.every((x) => x >= 0 && x <= 100)).toBe(true)
    w.unmount()
  })

  it('每个指标对每一行都取到有限数（无 NaN 泄漏到界面）', () => {
    const metrics = [
      'tokens', 'tokens_in', 'tokens_out', 'tokens_cache_read', 'tokens_cache_write',
      'tokens_reasoning', 'sessions', 'duration_ms', 'cost_total',
    ] as const
    for (const m of metrics) {
      for (const r of BY_CLI) {
        expect(Number.isFinite(metricValue(r, m)), `${m} 对 ${r.adapter} 非有限`).toBe(true)
      }
    }
  })
})
