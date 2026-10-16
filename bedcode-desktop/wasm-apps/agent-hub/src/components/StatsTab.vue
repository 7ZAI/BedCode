<script setup lang="ts">
/**
 * 使用统计看板（票据 06 改版）
 *
 * # 结构
 * 水位 + 时间窗 + 立即扫描 → 授权 / 降级横幅 → KPI 六卡 → 趋势主图
 * → 节奏热力图 + CLI 占比环 → 项目排行 + 模型排行 → 数据清空。
 *
 * # 与日志分区的关系（本次改版的核心）
 * 此前本分区挂了一份**同源**的「会话明细」列表，与日志分区的分页表格
 * 数据完全重复、游标语义还相反（追加 vs 按页），两个 tab 共享一套筛选
 * 条件互相干扰。现已删除：会话级明细**只在日志分区**存在，看板只回答
 * 「用量是多少、什么时候用的、分布在哪」这类聚合问题。
 *
 * # 指标口径
 * `token 总量 = 输入 + 输出 + 缓存读 + 缓存写`，**不含推理**（推理是输出的
 * 子集，见 `utils/format.ts::totalTokens`）。指标与维度切换全在前端，
 * 只有**时间窗**走 guest 参数（服务端切片）。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { CliId, CliStatRow, ModelStatRow, ProjectStatRow, StatsMetric } from '../types'
import type { UseUsageReturn } from '../composables/useUsage'
import type { StatsDays } from '../composables/useUsage'
import {
  abbreviateProject,
  cacheHitRate,
  formatCost,
  formatDuration,
  formatPercent,
  formatSessionTime,
  formatTokens,
  metricValue,
  totalTokens,
} from '../utils/format'
import StatsTrend from './StatsTrend.vue'
import StatsHeatmap from './StatsHeatmap.vue'
import StatsDonut from './StatsDonut.vue'
import type { DonutSlice } from './StatsDonut.vue'
import StatsBars from './StatsBars.vue'
import type { BarRow } from './StatsBars.vue'

const props = defineProps<{
  usage: UseUsageReturn
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const state = computed(() => props.usage.state.value)
const stats = computed(() => props.usage.stats.value)
const home = computed(() => state.value?.home ?? '')
const syncing = computed(() => state.value?.status === 'syncing')
const authRequired = computed(() => state.value?.status === 'auth-required')
const days = computed(() => props.usage.statsDays.value)

/** 时间窗 pills（0 = 全部；默认 30 天见 `useUsage.statsDays`） */
const RANGES: StatsDays[] = [7, 30, 90, 0]

// ==================== 全局指标（趋势 / 排行 / 占比共用一个选择） ====================

/**
 * 可选指标
 *
 * `tokens_reasoning` 单列而非进构成堆叠：它是输出的子集，堆进去会让
 * 「各段之和 = 总量」这条读图前提失效。
 */
const METRICS: StatsMetric[] = [
  'tokens',
  'sessions',
  'tokens_in',
  'tokens_out',
  'tokens_cache_read',
  'tokens_cache_write',
  'tokens_reasoning',
  'duration_ms',
  'cost_total',
]
const metric = ref<StatsMetric>('tokens')

/** 趋势图的构成模式（单指标 / 四段堆叠） */
const trendMode = ref<'total' | 'stack'>('total')

/** 按当前指标格式化（排行 / 环心共用） */
function fmtMetric(v: number): string {
  switch (metric.value) {
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

const total = computed(() => stats.value?.total ?? null)
const hasData = computed(() => (total.value?.sessions ?? 0) > 0)

// ==================== KPI 六卡 ====================

const avgTokensPerSession = computed(() =>
  total.value && total.value.sessions > 0 ? total.value ? totalTokens(total.value) / total.value.sessions : 0 : 0,
)
const avgDurationPerSession = computed(() =>
  total.value && total.value.sessions > 0 ? total.value.duration_ms / total.value.sessions : 0,
)
const avgCostPerSession = computed(() =>
  total.value && total.value.sessions > 0 && total.value.cost_total != null
    ? total.value.cost_total / total.value.sessions
    : null,
)
const hitRate = computed(() => cacheHitRate(total.value))

/** 数据区间（首末会话时间）；无时间戳时给显式占位而不是空 */
const rangeText = computed(() => {
  const t0 = total.value?.first_at
  const t1 = total.value?.last_at
  if (!t0 || !t1) return t('hub.st.kpi.noRange')
  return t('hub.st.kpi.range', { from: formatSessionTime(t0), to: formatSessionTime(t1) })
})

/** 在册 CLI（顺序即身份色槽位顺序；未采集的显示 0 参与占比） */
const CLI_IDS: CliId[] = ['claude', 'codex', 'opencode', 'pi']
const cliRows = computed<CliStatRow[]>(() => {
  const rows = stats.value?.byCli ?? []
  return CLI_IDS.map(
    (id) => rows.find((r) => r.adapter === id) ?? emptyCliRow(id),
  )
})
function emptyCliRow(adapter: string): CliStatRow {
  return {
    adapter,
    sessions: 0,
    tokens_in: 0,
    tokens_out: 0,
    tokens_cache_read: 0,
    tokens_cache_write: 0,
    tokens_reasoning: 0,
    duration_ms: 0,
    cost_total: null,
    last_at: null,
  }
}

/** 已解析条目数：遍历全部适配器求和（新增适配器自动纳入，勿写死家数） */
const parsedTotal = computed(() => {
  const adapters = state.value?.adapters
  if (!adapters) return 0
  return Object.values(adapters).reduce((sum, a) => sum + (a?.parsed ?? 0), 0)
})

// ==================== CLI 占比环 ====================

const donutSlices = computed<DonutSlice[]>(() =>
  cliRows.value.map((r, i) => ({
    key: r.adapter,
    label: r.adapter,
    value: metricValue(r, metric.value),
    colorIndex: i,
  })),
)
const donutCenter = computed(() => ({
  value: fmtMetric(metricValue(total.value ?? {}, metric.value)),
  label: t(`hub.st.metric.${metric.value}`),
}))

// ==================== 排行（项目 / 模型） ====================

/** 排行最多显示 8 行（超出部分用脚注说明总数，guest 侧只返回前 20） */
const BAR_LIMIT = 8

const projectRows = computed<BarRow[]>(() =>
  (stats.value?.byProject ?? []).slice(0, BAR_LIMIT).map((r: ProjectStatRow, i) => ({
    key: `p${i}-${r.project ?? ''}`,
    label: r.project ? abbreviateProject(r.project, home.value) : t('hub.st.noProject'),
    sub: `${t('hub.st.metric.sessions')} ${r.sessions}`,
    value: metricValue(r, metric.value),
    valueText: fmtMetric(metricValue(r, metric.value)),
  })),
)
const projectRest = computed(() => Math.max(0, (stats.value?.byProject ?? []).length - BAR_LIMIT))

const modelRows = computed<BarRow[]>(() =>
  (stats.value?.byModel ?? []).slice(0, BAR_LIMIT).map((r: ModelStatRow, i) => ({
    key: `m${i}-${r.model}`,
    label: r.model,
    sub: t('hub.st.modelMessages', { n: r.messages }),
    value: metricValue(r, metric.value),
    valueText: fmtMetric(metricValue(r, metric.value)),
  })),
)
const modelRest = computed(() => Math.max(0, (stats.value?.byModel ?? []).length - BAR_LIMIT))

// ==================== 降级提示与空态（票 07） ====================

/** 处于降级态的数据源（机器可读 code → i18n 文案，不透出 guest 原文） */
const degraded = computed(() => props.usage.adapterErrors.value)

/** 窗内无数据 ≠ 没数据：给出「扩大时间窗」出口，而不是让用户以为坏了 */
const emptyWindow = computed(() => !!stats.value && !hasData.value && days.value > 0)

// ==================== 数据清空（票 07） ====================

/** 两击确认：第一步只展开确认条，第二步才真发命令（防误触） */
const clearAsking = ref(false)
const clearDone = ref(false)
const clearFailed = ref(false)

async function onClearData() {
  clearDone.value = false
  clearFailed.value = false
  if (!clearAsking.value) {
    clearAsking.value = true
    return
  }
  clearAsking.value = false
  const r = await props.usage.clearData()
  if (r.ok) {
    clearDone.value = true
  } else {
    clearFailed.value = true
  }
}

function onClearCancel() {
  clearAsking.value = false
  clearFailed.value = false
}
</script>

<template>
  <div>
    <!-- ==================== 页头：水位 + 时间窗 + 立即扫描 ==================== -->
    <div class="ah-st-head">
      <span class="ah-cli-tag" :class="{ ok: !syncing }">
        {{ syncing ? t('hub.st.syncing') : t('hub.st.syncedTag', { n: parsedTotal }) }}
      </span>
      <div class="ah-st-ranges" role="group" :aria-label="t('hub.st.range')">
        <button
          v-for="r in RANGES"
          :key="r"
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm"
          :class="{ 'ah-btn-primary': days === r }"
          :aria-pressed="days === r"
          :data-testid="`range-${r}`"
          @click="usage.setStatsDays(r)"
        >
          {{ t(`hub.st.range.${r}`) }}
        </button>
      </div>
      <span class="ah-st-head-spacer"></span>
      <button
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        :disabled="syncing || authRequired"
        @click="usage.scan()"
      >
        {{ syncing ? t('hub.st.scanning') : t('hub.st.scanNow') }}
      </button>
    </div>

    <div v-if="authRequired" class="ah-banner">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.auth.banner') }}</span>
    </div>

    <!-- 适配器降级（票 07）：opencode 等源读不到时**显式说明原因**，
         不让「永远没数据」静默存在（spec §8 fail-visible） -->
    <div v-if="degraded.length > 0" class="ah-banner ah-banner-info" data-testid="usage-degraded">
      <span class="ah-banner-ic">ⓘ</span>
      <span class="ah-banner-text">
        <b>{{ t('hub.st.degraded') }}</b>
        <span v-for="d in degraded" :key="d.adapter" class="ah-st-degraded-row">
          <span class="ah-cli-tag neutral"><span class="ah-cli-dot"></span>{{ d.adapter }}</span>
          <span class="ah-st-degraded-text">{{ t(`hub.st.degraded.${d.code}`) }}</span>
        </span>
      </span>
    </div>

    <!-- ==================== KPI 六卡 ==================== -->
    <div v-if="hasData" class="ah-st-kpis" data-testid="kpi-row">
      <div class="ah-st-kpi">
        <div class="ah-st-kpi-val ah-mono">{{ total?.sessions }}</div>
        <div class="ah-st-kpi-label">{{ t('hub.st.kpi.sessions') }}</div>
        <div class="ah-st-kpi-sub">
          {{ t('hub.st.kpi.sessionsSub', { days: total?.active_days ?? 0, avg: formatTokens(avgTokensPerSession) }) }}
        </div>
      </div>
      <div class="ah-st-kpi">
        <div class="ah-st-kpi-val ah-mono">{{ formatTokens(totalTokens(total)) }}</div>
        <div class="ah-st-kpi-label">{{ t('hub.st.kpi.tokens') }}</div>
        <div class="ah-st-kpi-sub">
          {{ t('hub.st.kpi.tokensSub', { in: formatTokens(total?.tokens_in), out: formatTokens(total?.tokens_out) }) }}
        </div>
      </div>
      <div class="ah-st-kpi">
        <div class="ah-st-kpi-val ah-mono">{{ formatPercent(hitRate) }}</div>
        <div class="ah-st-kpi-label">{{ t('hub.st.kpi.cache') }}</div>
        <div class="ah-st-kpi-sub">
          {{
            t('hub.st.kpi.cacheSub', {
              r: formatTokens(total?.tokens_cache_read),
              w: formatTokens(total?.tokens_cache_write),
            })
          }}
        </div>
      </div>
      <div class="ah-st-kpi">
        <div class="ah-st-kpi-val ah-mono">{{ formatDuration(total?.duration_ms) }}</div>
        <div class="ah-st-kpi-label">{{ t('hub.st.kpi.duration') }}</div>
        <div class="ah-st-kpi-sub">
          {{ t('hub.st.kpi.durationSub', { avg: formatDuration(avgDurationPerSession) }) }}
        </div>
      </div>
      <div class="ah-st-kpi">
        <div class="ah-st-kpi-val ah-mono">{{ formatCost(total?.cost_total) }}</div>
        <div class="ah-st-kpi-label">{{ t('hub.st.kpi.cost') }}</div>
        <div class="ah-st-kpi-sub">
          {{ avgCostPerSession == null ? t('hub.st.kpi.costNone') : t('hub.st.kpi.costSub', { avg: formatCost(avgCostPerSession) }) }}
        </div>
      </div>
      <div class="ah-st-kpi">
        <div class="ah-st-kpi-val ah-mono">
          {{ t('hub.st.kpi.coverageVal', { clis: cliRows.filter((c) => c.sessions > 0).length }) }}
        </div>
        <div class="ah-st-kpi-label">{{ t('hub.st.kpi.coverage') }}</div>
        <div class="ah-st-kpi-sub">
          {{
            t('hub.st.kpi.coverageSub', {
              projects: total?.projects ?? 0,
              models: total?.models ?? 0,
            })
          }}
        </div>
        <div class="ah-st-kpi-sub ah-st-kpi-sub-dim">{{ rangeText }}</div>
      </div>
    </div>

    <!-- 窗内无数据：显式说明 + 给「看全部」出口（不静默空看板） -->
    <div v-if="emptyWindow" class="ah-card ah-st-nowindow" data-testid="empty-window">
      <div class="ah-st-empty">{{ t('hub.st.emptyWindow', { range: t(`hub.st.range.${days}`) }) }}</div>
      <button
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        data-testid="empty-window-all"
        @click="usage.setStatsDays(0)"
      >
        {{ t('hub.st.range.0') }}
      </button>
    </div>

    <!-- ==================== 指标选择（全局：趋势 / 排行 / 占比共用） ==================== -->
    <div class="ah-st-metrics" role="group" :aria-label="t('hub.st.metricLabel')">
      <button
        v-for="m in METRICS"
        :key="m"
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        :class="{ 'ah-btn-primary': metric === m }"
        :aria-pressed="metric === m"
        :data-testid="`metric-${m}`"
        @click="metric = m"
      >
        {{ t(`hub.st.metric.${m}`) }}
      </button>
    </div>

    <!-- ==================== 趋势主图 ==================== -->
    <div class="ah-card">
      <div class="ah-section-title">{{ t('hub.st.trend.title') }}</div>
      <StatsTrend
        :rows="stats?.byDay ?? []"
        :metric="metric"
        :mode="trendMode"
        @update:mode="trendMode = $event"
      />
    </div>

    <!-- ==================== 节奏热力图 + CLI 占比 ==================== -->
    <div class="ah-grid">
      <div class="ah-card">
        <div class="ah-section-title">{{ t('hub.st.heat.title') }}</div>
        <div class="ah-card-sub">{{ t('hub.st.heat.sub') }}</div>
        <StatsHeatmap :cells="stats?.byHour ?? []" />
      </div>

      <div class="ah-card">
        <div class="ah-section-title">{{ t('hub.st.donut.title') }}</div>
        <div class="ah-card-sub">{{ t('hub.st.donut.sub', { metric: t(`hub.st.metric.${metric}`) }) }}</div>
        <StatsDonut
          :slices="donutSlices"
          :center-value="donutCenter.value"
          :center-label="donutCenter.label"
        />
      </div>
    </div>

    <!-- ==================== 项目排行 + 模型排行 ==================== -->
    <div class="ah-grid">
      <div class="ah-card">
        <div class="ah-section-title">{{ t('hub.st.bars.projects') }}</div>
        <StatsBars :rows="projectRows" :empty-text="t('hub.st.bars.empty')" />
        <div v-if="projectRest > 0" class="ah-bars-rest">
          {{ t('hub.st.bars.more', { n: projectRest }) }}
        </div>
      </div>

      <div class="ah-card">
        <div class="ah-section-title">{{ t('hub.st.bars.models') }}</div>
        <StatsBars :rows="modelRows" :empty-text="t('hub.st.bars.empty')" />
        <div v-if="modelRest > 0" class="ah-bars-rest">
          {{ t('hub.st.bars.more', { n: modelRest }) }}
        </div>
      </div>
    </div>

    <!-- 数据清空（票 07）：保留策略为全量保留不自动过期，这是唯一的清理入口 -->
    <div class="ah-card ah-st-clear" data-testid="usage-clear">
      <div v-if="clearAsking" class="ah-st-clear-ask">
        <span class="ah-st-clear-text">{{ t('hub.st.clearDataAsk') }}</span>
        <span class="ah-st-clear-btns">
          <button
            type="button"
            class="ah-btn ah-btn-warn ah-btn-sm"
            :disabled="usage.clearing.value"
            @click="onClearData"
          >
            {{ t('hub.st.clearDataConfirm') }}
          </button>
          <button
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :disabled="usage.clearing.value"
            @click="onClearCancel"
          >
            {{ t('hub.st.clearDataCancel') }}
          </button>
        </span>
      </div>
      <button
        v-else
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        :disabled="usage.clearing.value"
        @click="onClearData"
      >
        {{ t('hub.st.clearData') }}
      </button>
      <span v-if="clearDone" class="ah-st-clear-note" role="status">
        {{ t('hub.st.clearDataDone') }}
      </span>
      <span v-else-if="clearFailed" class="ah-cli-error" role="alert">
        {{ t('hub.st.clearDataFailed') }}
      </span>
    </div>
  </div>
</template>
