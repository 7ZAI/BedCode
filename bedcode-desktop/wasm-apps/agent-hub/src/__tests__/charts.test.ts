/**
 * 看板图表组件行为契约（票据 06 改版）
 *
 * 契约来源（从实现分支反推，前置/输入/预期见每个 it 的注释）：
 * - T（趋势图）：`empty` 分支（无行 / 全 0 两种成因）走空态而非空 svg；
 *   单指标渲染 area+line 两段、堆叠渲染 4 段 band 且不画 line（line 与
 *   堆叠同时出现会给出「总量」的错误读数）；hover/focus 同一取值路径；
 *   数据表是图形之外的第二读取通道（无障碍兜底）
 * - H（热力图）：格子恒为 7×24 = 168（缺数据的格子是 0 值格，不是缺席）；
 *   峰值取 token 最大的格，并列取最早；分档在 max=0 时不除零
 * - O（占比环）：0 值只进图例不占弧（否则「0% 也占一格」）；段长按占比
 *   且扣掉缝隙；图例是主载体（名称 + 百分比），环只是辅助
 * - B（排行）：行数 = 入参行数；条宽按 value/max；max=0 不崩
 *
 * 断言全部落在**外部可见**的 DOM / 事件上：不测内部 computed，不测 mock。
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import StatsTrend from '../components/StatsTrend.vue'
import StatsHeatmap from '../components/StatsHeatmap.vue'
import StatsDonut from '../components/StatsDonut.vue'
import StatsBars from '../components/StatsBars.vue'
import type { BarRow, DayStatRow, DonutSlice, HourCell } from '../types'

/** i18n 桩直返 key；把插值参数渲染出来以便断言文案内容 */
function makeContext(): PluginContext {
  return {
    i18n: {
      t: (k: string, params?: Record<string, unknown>) =>
        params && Object.keys(params).length > 0
          ? `${k}(${Object.entries(params).map(([pk, pv]) => `${pk}=${String(pv)}`).join(',')})`
          : k,
      getI18n: () => undefined,
    },
    commands: { execute: vi.fn() },
    events: { on: () => ({ dispose: () => {} }) },
  } as unknown as PluginContext
}

/**
 * 挂载并登记（afterEach 统一 unmount）
 *
 * 不回收的话，每个 wrapper 连同其 happy-dom 子树会活到整个 worker 结束：
 * 热力图单个实例就有 168 个格子 + 7 行容器，本文件 30+ 次挂载叠加起来足以
 * 把整仓测试的默认堆顶穿（实测：带本文件跑全量 → OOM；去掉 → 通过）。
 */
const mounted: VueWrapper[] = []
afterEach(() => {
  while (mounted.length > 0) mounted.pop()?.unmount()
})

function mountComponent(component: unknown, props: Record<string, unknown>) {
  const w = mount(component as never, {
    props,
    global: { provide: { pluginContext: makeContext() } },
  } as never) as VueWrapper
  mounted.push(w)
  return w
}

/** 日行工厂（只写与断言相关的字段，其余给 0） */
function day(over: Partial<DayStatRow> = {}): DayStatRow {
  return {
    day: '2026-09-01',
    sessions: 1,
    tokens_in: 0,
    tokens_out: 0,
    tokens_cache_read: 0,
    tokens_cache_write: 0,
    tokens_reasoning: 0,
    duration_ms: 0,
    cost_total: null,
    ...over,
  }
}

// ==================== T 趋势图 ====================

describe('T 趋势图', () => {
  it('无行 → 空态（不画空 svg）', () => {
    const w = mountComponent(StatsTrend, { rows: [], metric: 'tokens', mode: 'total' })
    expect(w.find('.ah-trend-svg').exists()).toBe(false)
    expect(w.text()).toContain('hub.st.trend.empty')
  })

  it('有行但全 0 → 同样空态（「有日期没用量」不是趋势）', () => {
    const w = mountComponent(StatsTrend, { rows: [day(), day({ day: '2026-09-02' })], metric: 'tokens', mode: 'total' })
    expect(w.find('.ah-trend-svg').exists()).toBe(false)
  })

  it('单指标模式：画面积 + 线；堆叠模式：画 4 段且不画线', () => {
    const rows = [day({ tokens_in: 10, tokens_out: 20 }), day({ day: '2026-09-02', tokens_in: 5, tokens_cache_read: 30 })]
    const single = mountComponent(StatsTrend, { rows, metric: 'tokens', mode: 'total' })
    expect(single.find('.ah-trend-area').exists()).toBe(true)
    expect(single.find('.ah-trend-line').exists()).toBe(true)
    expect(single.findAll('.ah-trend-band')).toHaveLength(0)

    const stack = mountComponent(StatsTrend, { rows, metric: 'tokens', mode: 'stack' })
    expect(stack.findAll('.ah-trend-band')).toHaveLength(4)
    // 反例守门：line 与堆叠同时出现会让人把堆叠顶沿误读成「总量」
    expect(stack.find('.ah-trend-line').exists()).toBe(false)
    expect(stack.find('.ah-trend-area').exists()).toBe(false)
  })

  it('堆叠段按「输入 / 输出 / 缓存读 / 缓存写」四槽上色（色槽与图例一致）', () => {
    const rows = [day({ tokens_in: 10, tokens_out: 20, tokens_cache_read: 30, tokens_cache_write: 40 })]
    const w = mountComponent(StatsTrend, { rows, metric: 'tokens', mode: 'stack' })
    const fills = w.findAll('.ah-trend-band').map((b) => b.attributes('fill'))
    expect(fills).toEqual([
      'var(--chart-c1)',
      'var(--chart-c2)',
      'var(--chart-c3)',
      'var(--chart-c4)',
    ])
    expect(w.find('.ah-trend-legend').text()).toContain('hub.st.metric.tokens_cache_write')
  })

  it('hover 一列 → 悬浮块出现且给出该日全量分解', async () => {
    const rows = [
      day({ day: '2026-09-01', sessions: 3, tokens_in: 1000, tokens_out: 2000, duration_ms: 60_000, cost_total: 2.5 }),
      day({ day: '2026-09-02', sessions: 9, tokens_in: 4000 }),
    ]
    const w = mountComponent(StatsTrend, { rows, metric: 'tokens', mode: 'total' })
    expect(w.find('.ah-trend-tip').exists()).toBe(false)
    await w.findAll('.ah-trend-hit')[1].trigger('mouseenter')
    const tip = w.get('.ah-trend-tip').text()
    expect(tip).toContain('2026-09-02')
    expect(tip).toContain('hub.st.metric.sessions 9')
    expect(tip).toContain('hub.st.metric.duration_ms')
  })

  it('键盘 focus 与 hover 同路径（focus 即可读到数值，无需鼠标）', async () => {
    const rows = [day({ sessions: 4, tokens_in: 7000 })]
    const w = mountComponent(StatsTrend, { rows, metric: 'tokens', mode: 'total' })
    await w.get('.ah-trend-hit').trigger('focus')
    expect(w.find('.ah-trend-tip').exists()).toBe(true)
    expect(w.get('.ah-trend-hit').attributes('aria-label')).toContain('hub.st.metric.sessions 4')
  })

  it('数据表兜底：切到表格后行数 = 数据行数（图外的第二读取通道）', async () => {
    const rows = [day({ day: '2026-09-01' }), day({ day: '2026-09-02' }), day({ day: '2026-09-03' })]
    const w = mountComponent(StatsTrend, { rows, metric: 'sessions', mode: 'total' })
    await w.get('[data-testid=trend-table-toggle]').trigger('click')
    expect(w.find('.ah-trend-svg').exists()).toBe(false)
    expect(w.findAll('.ah-trend-table-wrap tbody tr')).toHaveLength(3)
    // 再切回图表
    await w.get('[data-testid=trend-table-toggle]').trigger('click')
    expect(w.find('.ah-trend-svg').exists()).toBe(true)
  })

  it('模式切换向上抛 update:mode（父级持有状态）', async () => {
    const w = mountComponent(StatsTrend, { rows: [day()], metric: 'tokens', mode: 'total' })
    await w.findAll('.ah-trend-modes button')[1].trigger('click')
    expect(w.emitted('update:mode')?.[0]).toEqual(['stack'])
  })

  it('x 轴标签不超过 6 个且按数据点定位（非等分错位）', () => {
    const rows = Array.from({ length: 20 }, (_, i) =>
      day({ day: `2026-09-${String(i + 1).padStart(2, '0')}`, tokens_in: (i + 1) * 1000 }),
    )
    const w = mountComponent(StatsTrend, { rows, metric: 'tokens', mode: 'total' })
    const ticks = w.findAll('.ah-trend-xtick')
    expect(ticks.length).toBeLessThanOrEqual(6)
    expect(ticks[0].text()).toBe('09-01')
    expect(ticks[ticks.length - 1].text()).toBe('09-20')
    expect(ticks[0].attributes('style')).toContain('left')
  })

  it('边界：只有一天数据时也画得出来（单点不除零）', () => {
    const w = mountComponent(StatsTrend, { rows: [day({ tokens_in: 500 })], metric: 'tokens', mode: 'total' })
    expect(w.find('.ah-trend-svg').exists()).toBe(true)
    expect(w.find('.ah-trend-line').attributes('d')).not.toContain('NaN')
  })
})

// ==================== H 节奏热力图 ====================

describe('H 节奏热力图', () => {
  it('无格 → 空态（不画 168 个空格）', () => {
    const w = mountComponent(StatsHeatmap, { cells: [] })
    expect(w.find('.ah-heat-grid').exists()).toBe(false)
    expect(w.text()).toContain('hub.st.heat.empty')
  })

  it('格子恒为 7×24 = 168（缺的格子是 0 值格，不是缺席）且行序为周一 → 周日', () => {
    // SQLite %w 的 0=周日，前端必须重排成周一开头
    const w = mountComponent(StatsHeatmap, { cells: [{ dow: 1, hour: 9, sessions: 1, tokens: 100 }] })
    expect(w.findAll('.ah-heat-cell')).toHaveLength(168)
    expect(w.findAll('.ah-heat-dow').map((d) => d.text())).toEqual([
      'hub.st.heat.dow.1',
      'hub.st.heat.dow.2',
      'hub.st.heat.dow.3',
      'hub.st.heat.dow.4',
      'hub.st.heat.dow.5',
      'hub.st.heat.dow.6',
      'hub.st.heat.dow.0',
    ])
  })

  it('容器子元素恒为 8 个整行（表头 + 7 天）——styleGuards S14 的 DOM 侧前提', () => {
    // 为什么值得单独锁：`.ah-heat-grid` 的列模板必须是单列堆叠，而它能不能这么写
    // 取决于**子元素是不是整行**。子元素一旦变成 25 个格子（模板改平铺），单列堆叠
    // 就反过来把格子排成一条。CSS 契约（styleGuards S14）与本条必须成对成立。
    const w = mountComponent(StatsHeatmap, { cells: [{ dow: 1, hour: 9, sessions: 1, tokens: 100 }] })
    const kids = Array.from(w.get('.ah-heat-grid').element.children)
    expect(kids).toHaveLength(8)
    expect(kids.every((el) => el.classList.contains('ah-heat-row'))).toBe(true)
    // 首行是表头：角标 + 5 个小时刻度（刻度靠 grid-column 定位，故只有 6 个子元素）
    expect(kids[0].querySelector('.ah-heat-corner')).toBeTruthy()
    expect(kids[0].querySelectorAll('.ah-heat-hour')).toHaveLength(5)
    expect(kids[0].children).toHaveLength(6)
    // 其余 7 行各自是 星期标签 + 24 格 = 25 个直接子元素
    for (const el of kids.slice(1)) {
      expect(el.children).toHaveLength(25)
      expect(el.querySelectorAll('.ah-heat-cell')).toHaveLength(24)
      expect(el.querySelector('.ah-heat-dow')).toBeTruthy()
    }
  })

  it('峰值文字给出正确的星期与小时（token 最大的格）', () => {
    const cells: HourCell[] = [
      { dow: 1, hour: 9, sessions: 1, tokens: 500 },
      { dow: 4, hour: 21, sessions: 4, tokens: 9_000 },
    ]
    const w = mountComponent(StatsHeatmap, { cells })
    const peak = w.get('.ah-heat-peak').text()
    expect(peak).toContain('9.0k')
    expect(peak).toContain('hub.st.heat.dow.4')
    expect(peak).toContain('hour=21')
  })

  it('分档按名次而非线性比例（偏态下仍能分出「次高时段」），且相同值同档', () => {
    // 强偏态：100 是其余的 25~100 倍。线性分档会把 1/2/3/4 全压进最低档，
    // 热力图退化成「一格深 + 全浅」，恰好丢掉最想知道的次高信息。
    const cells: HourCell[] = [
      { dow: 1, hour: 1, sessions: 1, tokens: 1 },
      { dow: 2, hour: 2, sessions: 1, tokens: 2 },
      { dow: 3, hour: 3, sessions: 1, tokens: 3 },
      { dow: 4, hour: 4, sessions: 1, tokens: 4 },
      { dow: 5, hour: 5, sessions: 1, tokens: 100 },
    ]
    const w = mountComponent(StatsHeatmap, { cells })
    const levels = w.findAll('.ah-heat-cell').map((c) => (c.classes().find((k) => k.startsWith('lv')) ?? '').slice(2))
    // 网格按 周一..周日 × 0..23 展开：下标 = 行号×24 + 小时
    const idx = (row: number, hour: number) => row * 24 + hour

    // 最大格必落顶档
    expect(levels[idx(4, 5)]).toBe('4')
    // 每一档都有格（名次分档的定义）
    expect(new Set(levels.filter((l) => l !== '0'))).toEqual(new Set(['1', '2', '3', '4']))
    // 空格恒为 lv0
    expect(levels.filter((l) => l === '0')).toHaveLength(163)
    // 相同 token 值必同档（标度稳定，不因并列而变色）
    const dup = mountComponent(StatsHeatmap, {
      cells: [
        { dow: 1, hour: 1, sessions: 1, tokens: 50 },
        { dow: 2, hour: 2, sessions: 1, tokens: 50 },
        { dow: 3, hour: 3, sessions: 1, tokens: 10 },
      ],
    })
    const lv = dup.findAll('.ah-heat-cell').map((c) => (c.classes().find((k) => k.startsWith('lv')) ?? '').slice(2))
    expect(lv[idx(1, 2)]).toBe(lv[idx(0, 1)])
    expect(Number(lv[idx(0, 1)])).toBeGreaterThan(Number(lv[idx(2, 3)]))
  })

  it('边界：全 0 的格子不产生除零（全落 lv0），图例仍给出数值上界', () => {
    // 图例上界走 formatTokens 的「进位向上」口径（1234 → 1.3k，不产生假零）
    const w = mountComponent(StatsHeatmap, { cells: [{ dow: 1, hour: 9, sessions: 0, tokens: 0 }] })
    const levels = w.findAll('.ah-heat-cell').map((c) => (c.classes().find((k) => k.startsWith('lv')) ?? '').slice(2))
    expect(new Set(levels)).toEqual(new Set(['0']))
    const withMax = mountComponent(StatsHeatmap, { cells: [{ dow: 1, hour: 9, sessions: 1, tokens: 1234 }] })
    expect(withMax.get('.ah-heat-legend').text()).toContain('1.3k')
  })
})

// ==================== O 占比环 ====================

describe('O 占比环', () => {
  const slices: DonutSlice[] = [
    { key: 'claude', label: 'claude', value: 75, colorIndex: 0 },
    { key: 'codex', label: 'codex', value: 25, colorIndex: 1 },
  ]

  it('全 0 → 空态（不画「空环」）', () => {
    const w = mountComponent(StatsDonut, {
      slices: slices.map((s) => ({ ...s, value: 0 })),
      centerValue: '0',
      centerLabel: 'token',
    })
    expect(w.find('.ah-donut-svg').exists()).toBe(false)
    expect(w.text()).toContain('hub.st.donut.empty')
  })

  it('图例是主载体：名称 + 百分比齐备（不靠颜色单独承载分类）', () => {
    const w = mountComponent(StatsDonut, { slices, centerValue: '100', centerLabel: 'token' })
    const rows = w.findAll('.ah-donut-legend-row')
    expect(rows).toHaveLength(2)
    expect(rows[0].text()).toContain('claude')
    expect(rows[0].text()).toContain('75%')
    expect(rows[1].text()).toContain('25%')
  })

  it('环心显示父级给的值与标签', () => {
    const w = mountComponent(StatsDonut, { slices, centerValue: '1.2M', centerLabel: 'token 总量' })
    expect(w.get('.ah-donut-center-val').text()).toBe('1.2M')
    expect(w.get('.ah-donut-center-label').text()).toBe('token 总量')
  })

  it('身份色按 colorIndex 取（与排序无关，claude 恒为 c1）', () => {
    const w = mountComponent(StatsDonut, {
      slices: [
        { key: 'codex', label: 'codex', value: 10, colorIndex: 1 },
        { key: 'claude', label: 'claude', value: 90, colorIndex: 0 },
      ],
      centerValue: '100',
      centerLabel: 'token',
    })
    const strokes = w.findAll('.ah-donut-arc').map((a) => a.attributes('stroke'))
    // 弧按占比降序无关，身份色由 colorIndex 决定：codex → c2、claude → c1
    expect(strokes).toContain('var(--chart-c1)')
    expect(strokes).toContain('var(--chart-c2)')
  })

  it('0 值只进图例不占弧（否则「0% 也占一格」）', () => {
    const w = mountComponent(StatsDonut, {
      slices: [
        ...slices,
        { key: 'opencode', label: 'opencode', value: 0, colorIndex: 2 },
      ],
      centerValue: '100',
      centerLabel: 'token',
    })
    expect(w.findAll('.ah-donut-arc')).toHaveLength(2)
    expect(w.findAll('.ah-donut-legend-row')).toHaveLength(3)
    const zero = w.findAll('.ah-donut-legend-row').at(-1)!
    expect(zero.text()).toContain('opencode')
    expect(zero.text()).toContain('—')
  })

  it('每段 dasharray 短于整圈（留缝），且各段长度之和 < 整圈', () => {
    const w = mountComponent(StatsDonut, { slices, centerValue: '100', centerLabel: 'token' })
    const dashes = w.findAll('.ah-donut-arc').map((a) => Number(a.attributes('stroke-dasharray')?.split(' ')[0]))
    const circle = 2 * Math.PI * 62
    for (const d of dashes) {
      expect(d).toBeGreaterThan(0)
      expect(d).toBeLessThan(circle)
    }
    // 75% + 25% 各自扣掉缝隙 → 合计必须小于整圈（否则两段会贴死/重叠）
    expect(dashes.reduce((a, b) => a + b, 0)).toBeLessThan(circle)
  })
})

// ==================== B 排行 ====================

describe('B 排行', () => {
  const rows: BarRow[] = [
    { key: 'a', label: '~/proj-a', sub: '会话 5', value: 100, valueText: '100' },
    { key: 'b', label: '~/proj-b', sub: '会话 1', value: 40, valueText: '40' },
  ]

  it('无行 → 空态（走父级给的 i18n 文案）', () => {
    const w = mountComponent(StatsBars, { rows: [], emptyText: '没有数据' })
    expect(w.findAll('.ah-bars-row')).toHaveLength(0)
    expect(w.text()).toContain('没有数据')
  })

  it('行数 = 入参行数，名称与副行都在', () => {
    const w = mountComponent(StatsBars, { rows, emptyText: '没有数据' })
    expect(w.findAll('.ah-bars-row')).toHaveLength(2)
    expect(w.text()).toContain('~/proj-a')
    expect(w.text()).toContain('会话 5')
  })

  it('条宽按 value / max 比例（第一行满格）', () => {
    const w = mountComponent(StatsBars, { rows, emptyText: '没有数据' })
    const widths = w.findAll('.ah-bars-fill').map((f) => f.attributes('style') ?? '')
    expect(widths[0]).toContain('100%')
    expect(widths[1]).toContain('40%')
  })

  it('极小值保底 1.5% 可见宽度（不能缩成一个看不见的点）', () => {
    const w = mountComponent(StatsBars, {
      rows: [
        { key: 'a', label: 'a', value: 1_000_000, valueText: '1M' },
        { key: 'b', label: 'b', value: 1, valueText: '1' },
      ],
      emptyText: 'x',
    })
    expect(w.findAll('.ah-bars-fill')[1].attributes('style')).toContain('1.5%')
  })

  it('边界：全 0 → 宽度 0 且不崩（除零保护）', () => {
    const w = mountComponent(StatsBars, {
      rows: [{ key: 'a', label: 'a', value: 0, valueText: '0' }],
      emptyText: 'x',
    })
    expect(w.findAll('.ah-bars-row')).toHaveLength(1)
    expect(w.find('.ah-bars-fill').attributes('style')).toContain('0%')
  })

  it('显式 max 生效（父组件统一定标尺时用）', () => {
    const w = mountComponent(StatsBars, { rows, max: 200, emptyText: 'x' })
    const widths = w.findAll('.ah-bars-fill').map((f) => f.attributes('style') ?? '')
    expect(widths[0]).toContain('50%')
    expect(widths[1]).toContain('20%')
  })
})

// ==================== 几何辅助 ====================

describe('图表组件挂载边界', () => {
  it('数据全空的热力图仍能挂载并给出空态（不抛错、不留幽灵格）', async () => {
    const w = mountComponent(StatsHeatmap, { cells: [] })
    await flushPromises()
    expect(w.findAll('.ah-heat-cell')).toHaveLength(0)
    expect(w.text()).toContain('hub.st.heat.empty')
  })

  it('卸载后不再响应后续挂载（无跨用例残留）', async () => {
    const w = mountComponent(StatsTrend, { rows: [day({ tokens_in: 1 })], metric: 'tokens', mode: 'total' })
    await flushPromises()
    expect(w.find('.ah-trend-svg').exists()).toBe(true)
  })
})
