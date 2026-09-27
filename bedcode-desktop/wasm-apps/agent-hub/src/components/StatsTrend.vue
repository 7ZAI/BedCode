<script setup lang="ts">
/**
 * 趋势图（看板主图）——按天的单指标面积图 / 四段构成堆叠
 *
 * 设计决策（检索来源：ui-ux-pro-max `--domain chart`「Trend Over Time」
 * → Line/Area Chart，**risk:low**；配套「part-to-whole」→ Stacked Bar）：
 * - 时间轴上的连续量用**面积 + 线**（不是逐日横条：横条读不出趋势，只读得出
 *   单日大小，30 天时更退化成一张列表）
 * - 构成按**固定语义序**堆叠（输入 → 输出 → 缓存读 → 缓存写），不按当日大小
 *   重排：逐日重排会让色带在图上左右乱窜，读者无法追踪任一色带
 * - 每段描 1px 卡片底色描边，使「哪段是哪段」不只依赖色相
 *   （同源既有约定：`.ah-st-cbar` 的 1px gap 露轨道底色）
 * - 交互层是**HTML 覆盖列**而非 SVG 命中测试：每列一个可聚焦按钮，
 *   键盘 focus 与鼠标 hover 走同一条取值路径，天然满足「focus 揭示数值」
 * - 无障碍兜底：图形 `aria-hidden`，同一份数据有可切换的 `<table>` 呈现
 *   （检索建议的 A11y Fallback：visible data table）
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { DayStatRow, StatsMetric } from '../types'
import {
  axisTicks,
  formatCost,
  formatDuration,
  formatTokens,
  metricValue,
  niceMax,
} from '../utils/format'

// `mode`：total = 单指标面积图；stack = 四段 token 构成堆叠
// （注释放在类型字面量**外面**：宏的类型参数扫描不跳注释，内嵌注释会报解析错）
const props = defineProps<{
  rows: DayStatRow[]
  metric: StatsMetric
  mode: 'total' | 'stack'
}>()

const emit = defineEmits<{ 'update:mode': [mode: 'total' | 'stack'] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

// ==================== 几何（固定 viewBox + 等比缩放，描边用 non-scaling-stroke） ====================

/**
 * 画布几何（写成一个字面量对象而非三个相互引用的 const）
 *
 * SFC 编译器会把「字面量初始化的 const」提升到模块作用域，而引用其它
 * 提升量的 const（如 `const BASE = H - 2`）留在 setup 里——两者混用会让
 * setup 内出现对未定义提升量的引用（编译产物里报 `H is not defined`）。
 * 合并成一个字面量就没有跨作用域引用。
 */
const GEO = { w: 720, h: 180, /** 基线（底部留 2px，避免描边被裁） */ base: 178 } as const

/** 构成堆叠的固定语义序（自下而上）；色槽 c1..c4 与图例一一对应 */
const STACK_SERIES = [
  { key: 'tokens_in', color: 'var(--chart-c1)' },
  { key: 'tokens_out', color: 'var(--chart-c2)' },
  { key: 'tokens_cache_read', color: 'var(--chart-c3)' },
  { key: 'tokens_cache_write', color: 'var(--chart-c4)' },
] as const

const n = computed(() => props.rows.length)

/** 单指标序列值（stack 模式也用它定上界，保证两模式切换不跳轴） */
const values = computed(() =>
  props.rows.map((r) => {
    if (props.mode === 'stack') {
      return (
        (r.tokens_in || 0) +
        (r.tokens_out || 0) +
        (r.tokens_cache_read || 0) +
        (r.tokens_cache_write || 0)
      )
    }
    return metricValue(r, props.metric)
  }),
)

const max = computed(() => niceMax(Math.max(0, ...values.value)))
const ticks = computed(() => axisTicks(max.value, 3))

function x(i: number): number {
  if (n.value <= 1) return GEO.w / 2
  return (i * GEO.w) / (n.value - 1)
}
function y(v: number): number {
  if (max.value <= 0) return GEO.base
  return GEO.base - (v / max.value) * (GEO.base - 6)
}

const empty = computed(() => n.value === 0 || values.value.every((v) => v === 0))

/** 单指标：面积（渐变填充）+ 线 */
const areaPath = computed(() => {
  if (empty.value) return ''
  const pts = values.value.map((v, i) => `${x(i).toFixed(2)},${y(v).toFixed(2)}`)
  return `M ${pts[0]} L ${pts.slice(1).join(' L ')} L ${x(n.value - 1).toFixed(2)},${GEO.base} L ${x(0).toFixed(2)},${GEO.base} Z`
})
const linePath = computed(() => {
  if (empty.value) return ''
  return `M ${values.value
    .map((v, i) => `${x(i).toFixed(2)},${y(v).toFixed(2)}`)
    .join(' L ')}`
})

/** 构成堆叠：逐段「上沿 − 下沿」的闭合带（上沿自下而上累加） */
interface Band {
  color: string
  d: string
}
const bands = computed<Band[]>(() => {
  if (props.mode !== 'stack' || empty.value) return []
  const cum = values.value.map(() => 0)
  return STACK_SERIES.map((s) => {
    const top = props.rows.map((r, i) => {
      cum[i] += r[s.key] || 0
      return cum[i]
    })
    const lower = top.map((v, i) => v - (props.rows[i][s.key] || 0))
    const up = top.map((v, i) => `${x(i).toFixed(2)},${y(v).toFixed(2)}`)
    const down = lower
      .map((v, i) => `${x(i).toFixed(2)},${y(v).toFixed(2)}`)
      .reverse()
    return {
      color: s.color,
      d: `M ${up[0]} L ${up.slice(1).join(' L ')} L ${down.join(' L ')} Z`,
    }
  })
})

// ==================== 数值格式化（按指标类型选单位） ====================

function fmt(v: number): string {
  switch (props.metric) {
    case 'cost_total':
      return formatCost(v)
    case 'duration_ms':
      return formatDuration(v)
    case 'sessions':
      return String(Math.round(v))
    default:
      return formatTokens(v)
  }
}

// ==================== 交互（hover / focus 共用一条路径） ====================

const hover = ref<number | null>(null)
const hoveredRow = computed(() => (hover.value == null ? null : props.rows[hover.value]))
const hoveredValue = computed(() => (hover.value == null ? 0 : values.value[hover.value]))

/** 悬浮块横向位置（按列中心百分比定位，CSS 里做边界收拢） */
const tipLeft = computed(() => {
  if (hover.value == null || n.value <= 1) return '50%'
  return `${(x(hover.value) / GEO.w) * 100}%`
})

/** 每列的无障碍标签（键盘 focus 读到的是完整一行，不只是图形） */
function colLabel(r: DayStatRow): string {
  return `${r.day} ${t(`hub.st.metric.${props.metric}`)} ${fmt(metricValue(r, props.metric))} · ${t('hub.st.metric.sessions')} ${r.sessions}`
}

/**
 * x 轴标签：最多 6 个，均匀抽样（含首尾）
 *
 * 位置按 SVG 里的同一套 x() 换算成百分比（而非 flex 均匀分布）：
 * flex 只能均分**项间**距，项宽不同则整体对不上数据点。
 */
const xLabels = computed(() => {
  if (n.value === 0) return []
  const want = Math.min(6, n.value)
  const step = n.value > 1 ? (n.value - 1) / (want - 1 || 1) : 1
  const out: { i: number; text: string; left: string }[] = []
  for (let k = 0; k < want; k++) {
    const i = Math.round(k * step)
    if (out.some((o) => o.i === i)) continue
    out.push({ i, text: props.rows[i].day.slice(5), left: `${(x(i) / GEO.w) * 100}%` })
  }
  return out
})

// ==================== 数据表兜底（图形之外的第二读取通道） ====================

const tableMode = ref(false)
</script>

<template>
  <div class="ah-trend">
    <div class="ah-trend-modes">
      <button
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        :class="{ 'ah-btn-primary': mode === 'total' }"
        @click="emit('update:mode', 'total')"
      >
        {{ t('hub.st.trend.mode.total') }}
      </button>
      <button
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        :class="{ 'ah-btn-primary': mode === 'stack' }"
        @click="emit('update:mode', 'stack')"
      >
        {{ t('hub.st.trend.mode.stack') }}
      </button>
      <button
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        data-testid="trend-table-toggle"
        @click="tableMode = !tableMode"
      >
        {{ tableMode ? t('hub.st.trend.chart') : t('hub.st.trend.table') }}
      </button>
    </div>

    <div v-if="empty" class="ah-st-empty">{{ t('hub.st.trend.empty') }}</div>

    <div v-else-if="tableMode" class="ah-trend-table-wrap">
      <table class="ah-st-table">
        <thead>
          <tr>
            <th>{{ t('hub.st.trend.col.day') }}</th>
            <th class="num">{{ t(`hub.st.metric.${metric}`) }}</th>
            <th class="num">{{ t('hub.st.metric.sessions') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rows" :key="r.day">
            <td class="ah-mono">{{ r.day }}</td>
            <td class="num ah-mono">{{ fmt(metricValue(r, metric)) }}</td>
            <td class="num ah-mono">{{ r.sessions }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-else class="ah-trend-plot">
      <div class="ah-trend-yaxis">
        <span v-for="tick in ticks" :key="tick" class="ah-trend-ytick ah-mono">{{ fmt(tick) }}</span>
      </div>
      <div class="ah-trend-graphic">
        <svg
          class="ah-trend-svg"
          :viewBox="`0 0 ${GEO.w} ${GEO.h}`"
          preserveAspectRatio="none"
          role="presentation"
          aria-hidden="true"
        >
          <line
            v-for="tick in ticks"
            :key="`g${tick}`"
            class="ah-trend-grid"
            x1="0"
            :y1="y(tick)"
            :x2="GEO.w"
            :y2="y(tick)"
          />
          <path v-if="mode === 'total'" :d="areaPath" class="ah-trend-area" />
          <path
            v-for="(b, i) in bands"
            :key="`b${i}`"
            :d="b.d"
            :fill="b.color"
            class="ah-trend-band"
          />
          <path
            v-if="mode === 'total'"
            :d="linePath"
            class="ah-trend-line"
            stroke="var(--chart-c1)"
          />
        </svg>
        <div class="ah-trend-hits">
          <button
            v-for="(r, i) in rows"
            :key="r.day"
            type="button"
            class="ah-trend-hit"
            :class="{ on: hover === i }"
            :data-testid="`trend-hit-${i}`"
            :aria-label="colLabel(r)"
            @mouseenter="hover = i"
            @focus="hover = i"
            @blur="hover = null"
            @click="hover = hover === i ? null : i"
          ></button>
        </div>
        <div
          v-if="hoveredRow"
          class="ah-trend-tip"
          data-testid="trend-tip"
          :style="{ left: tipLeft }"
        >
          <div class="ah-trend-tip-day ah-mono">{{ hoveredRow.day }}</div>
          <div class="ah-trend-tip-main">
            <b class="ah-mono">{{ fmt(hoveredValue) }}</b>
            <span class="ah-trend-tip-unit">{{ t(`hub.st.metric.${metric}`) }}</span>
          </div>
          <div class="ah-trend-tip-rows ah-mono">
            <span>{{ t('hub.st.metric.sessions') }} {{ hoveredRow.sessions }}</span>
            <span>{{ t('hub.st.metric.tokens_in') }} {{ formatTokens(hoveredRow.tokens_in) }}</span>
            <span>{{ t('hub.st.metric.tokens_out') }} {{ formatTokens(hoveredRow.tokens_out) }}</span>
            <span>{{ t('hub.st.metric.tokens_cache_read') }} {{ formatTokens(hoveredRow.tokens_cache_read) }}</span>
            <span>{{ t('hub.st.metric.duration_ms') }} {{ formatDuration(hoveredRow.duration_ms) }}</span>
            <span v-if="hoveredRow.cost_total != null">{{ t('hub.st.metric.cost_total') }} {{ formatCost(hoveredRow.cost_total) }}</span>
          </div>
        </div>
      </div>
    </div>

    <div v-if="!empty && !tableMode" class="ah-trend-xaxis">
      <span
        v-for="l in xLabels"
        :key="l.i"
        class="ah-trend-xtick ah-mono"
        :style="{ left: l.left }"
        >{{ l.text }}</span
      >
    </div>

    <div class="ah-trend-legend">
      <template v-if="mode === 'stack'">
        <span v-for="s in STACK_SERIES" :key="s.key">
          <span class="ah-st-k" :style="{ background: s.color }" />
          {{ t(`hub.st.metric.${s.key}`) }}
        </span>
      </template>
      <span v-else>
        <span class="ah-st-k" style="background: var(--chart-c1)" />
        {{ t(`hub.st.metric.${metric}`) }}
      </span>
    </div>
  </div>
</template>
