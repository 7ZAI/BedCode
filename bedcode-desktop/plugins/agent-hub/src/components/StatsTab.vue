<script setup lang="ts">
/**
 * 使用统计分区（票据 06）
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` 变体 B「使用统计」
 * 页（页头水位 tag + 立即扫描；每日 tokens 堆叠横条 + 汇总表 grid2；维度
 * pills 按天/CLI/项目/模型切换汇总表分组；会话级明细列表分页加载）。
 * 数据经 useUsage 单实例：stats 一次拉全部分组，维度切换纯前端；扫描完成
 * 经 `plugin:agent-hub:usage` 事件回流后自动重拉。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { GroupStatRow, UsageSessionRow } from '../types'
import type { UseUsageReturn } from '../composables/useUsage'
import {
  abbreviateProject,
  formatCost,
  formatDuration,
  formatSessionTime,
  formatTokens,
} from '../utils/format'

const props = defineProps<{
  usage: UseUsageReturn
}>()

const emit = defineEmits<{ 'goto-logs': [sessionId: number] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const state = computed(() => props.usage.state.value)
const stats = computed(() => props.usage.stats.value)
const home = computed(() => state.value?.home ?? '')
const syncing = computed(() => state.value?.status === 'syncing')
const authRequired = computed(() => state.value?.status === 'auth-required')

/** 汇总表维度：day / cli / project / model（切换纯前端，分组数据已全量拉取） */
type Dimension = 'day' | 'cli' | 'project' | 'model'
const dimension = ref<Dimension>('cli')
function setDimension(d: Dimension) {
  dimension.value = d
}

// ==================== 每日堆叠条（输入/输出两段，宽度按全局最大占比） ====================

const dayRows = computed(() => {
  const days = stats.value?.byDay ?? []
  const max = Math.max(1, ...days.map((d) => (d.tokens_in || 0) + (d.tokens_out || 0)))
  return days.slice(-30).map((d) => ({
    day: d.day,
    total: (d.tokens_in || 0) + (d.tokens_out || 0),
    in: d.tokens_in || 0,
    out: d.tokens_out || 0,
    inPct: ((d.tokens_in || 0) / max) * 100,
    outPct: ((d.tokens_out || 0) / max) * 100,
  }))
})

// ==================== 汇总表（当前维度） ====================

interface SummaryRow {
  label: string
  sub?: string
  sessions: number
  duration: string
  tokensIn: number
  tokensOut: number
  cost: string
}

const summaryRows = computed<SummaryRow[]>(() => {
  const s = stats.value
  if (!s) return []
  const groupRow = (label: string, sub: string | undefined, r: GroupStatRow): SummaryRow => ({
    label,
    sub,
    sessions: r.sessions,
    duration: formatDuration(r.duration_ms),
    tokensIn: r.tokens_in || 0,
    tokensOut: r.tokens_out || 0,
    cost: formatCost(r.cost_total),
  })
  switch (dimension.value) {
    case 'day':
      return (s.byDay ?? []).map((d) => ({
        label: d.day,
        sessions: d.sessions,
        duration: '—',
        tokensIn: d.tokens_in || 0,
        tokensOut: d.tokens_out || 0,
        cost: '—',
      }))
    case 'cli':
      return (s.byCli ?? []).map((r) => groupRow(r.adapter, undefined, r))
    case 'project':
      return (s.byProject ?? []).map((r) => {
        const label =
          r.project && r.project.length > 0
            ? abbreviateProject(r.project, home.value)
            : t('hub.st.noProject')
        return groupRow(label, undefined, r)
      })
    case 'model':
      return (s.byModel ?? []).map((m) => ({
        label: m.model,
        sub: t('hub.st.modelMessages', { n: m.messages }),
        sessions: m.sessions,
        duration: '—',
        tokensIn: m.tokens_in || 0,
        tokensOut: m.tokens_out || 0,
        cost: '—',
      }))
    default:
      return []
  }
})

const total = computed(() => stats.value?.total)

// ==================== 会话明细列表（与日志视图共用分页数据） ====================

const sessions = computed<UsageSessionRow[]>(() => props.usage.sessions.value)
const hasMore = computed(() => sessions.value.length < props.usage.sessionsTotal.value)

function sessionLabel(s: UsageSessionRow): string {
  return s.title || s.cli_session_id
}
</script>

<template>
  <div>
    <div class="ah-st-head">
      <span class="ah-cli-tag" :class="{ ok: !syncing }">
        {{ syncing ? t('hub.st.syncing') : t('hub.st.syncedTag', { n: (state?.adapters?.claude?.parsed ?? 0) + (state?.adapters?.pi?.parsed ?? 0) }) }}
      </span>
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

    <div v-if="stats && total" class="ah-st-totals">
      <div class="ah-st-total">
        <div class="ah-st-total-val">{{ total.sessions }}</div>
        <div class="ah-st-total-label">{{ t('hub.st.totalSessions') }}</div>
      </div>
      <div class="ah-st-total">
        <div class="ah-st-total-val">{{ formatTokens((total.tokens_in || 0) + (total.tokens_out || 0)) }}</div>
        <div class="ah-st-total-label">{{ t('hub.st.totalTokens') }}</div>
      </div>
      <div class="ah-st-total">
        <div class="ah-st-total-val">{{ formatDuration(total.duration_ms) }}</div>
        <div class="ah-st-total-label">{{ t('hub.st.totalDuration') }}</div>
      </div>
      <div class="ah-st-total">
        <div class="ah-st-total-val">{{ formatCost(total.cost_total) }}</div>
        <div class="ah-st-total-label">{{ t('hub.st.totalCost') }}</div>
      </div>
    </div>

    <div class="ah-st-dims" role="tablist">
      <button
        v-for="d in (['day', 'cli', 'project', 'model'] as const)"
        :key="d"
        type="button"
        class="ah-btn ah-btn-ghost ah-btn-sm"
        :class="{ 'ah-btn-primary': dimension === d }"
        role="tab"
        :aria-selected="dimension === d"
        @click="setDimension(d)"
      >
        {{ t(`hub.st.dim.${d}`) }}
      </button>
    </div>

    <div class="ah-grid">
      <div class="ah-card">
        <div class="ah-section-title">{{ t('hub.st.dailyChart') }}</div>
        <div v-if="dayRows.length === 0" class="ah-st-empty">{{ t('hub.st.empty') }}</div>
        <div v-else class="ah-st-chart" aria-hidden="true">
          <div v-for="row in dayRows" :key="row.day" class="ah-st-crow" :title="t('hub.st.dayTip', { in: formatTokens(row.in), out: formatTokens(row.out) })">
            <span class="ah-st-clabel">{{ row.day.slice(5) }}</span>
            <div class="ah-st-cbar">
              <div class="ah-st-cin" :style="{ width: `${row.inPct}%` }" />
              <div class="ah-st-cout" :style="{ width: `${row.outPct}%` }" />
            </div>
            <span class="ah-st-cval">{{ formatTokens(row.total) }}</span>
          </div>
        </div>
        <div class="ah-st-legend">
          <span><span class="ah-st-k in" />{{ t('hub.st.legendIn') }}</span>
          <span><span class="ah-st-k out" />{{ t('hub.st.legendOut') }}</span>
        </div>
      </div>

      <div class="ah-card">
        <div class="ah-section-title">{{ t('hub.st.summaryBy', { dim: t(`hub.st.dim.${dimension}`) }) }}</div>
        <div v-if="summaryRows.length === 0" class="ah-st-empty">{{ t('hub.st.empty') }}</div>
        <table v-else class="ah-st-table">
          <thead>
            <tr>
              <th>{{ t('hub.st.col.name') }}</th>
              <th class="num">{{ t('hub.st.col.sessions') }}</th>
              <th class="num">{{ t('hub.st.col.tokensIn') }}</th>
              <th class="num">{{ t('hub.st.col.tokensOut') }}</th>
              <th class="num">{{ t('hub.st.col.cost') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in summaryRows" :key="r.label">
              <td>
                <div class="ah-st-name">{{ r.label }}</div>
                <div v-if="r.sub" class="ah-st-sub">{{ r.sub }}</div>
              </td>
              <td class="num">{{ r.sessions }}</td>
              <td class="num ah-mono">{{ formatTokens(r.tokensIn) }}</td>
              <td class="num ah-mono">{{ formatTokens(r.tokensOut) }}</td>
              <td class="num ah-mono">{{ r.cost }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="ah-card ah-st-detail">
      <div class="ah-section-title">{{ t('hub.st.sessionsTitle') }}</div>
      <div v-if="sessions.length === 0" class="ah-st-empty">{{ t('hub.st.empty') }}</div>
      <div
        v-for="s in sessions"
        :key="`${s.adapter}-${s.id}`"
        class="ah-st-row"
        role="button"
        tabindex="0"
        @click="emit('goto-logs', s.id)"
        @keydown.enter="emit('goto-logs', s.id)"
      >
        <div class="ah-st-row-main">
          <span class="ah-cli-tag ok">{{ s.adapter }}</span>
          <span class="ah-st-row-title">{{ sessionLabel(s) }}</span>
        </div>
        <div class="ah-st-row-meta ah-mono">
          <span>{{ formatSessionTime(s.started_at) }}</span>
          <span>{{ formatDuration(s.duration_ms) }}</span>
          <span>↑ {{ formatTokens(s.tokens_in) }} ↓ {{ formatTokens(s.tokens_out) }}</span>
          <span class="ah-st-row-project">{{ abbreviateProject(s.project, home) }}</span>
        </div>
      </div>
      <div v-if="hasMore" class="ah-st-more">
        <button
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm"
          :disabled="usage.loadingSessions.value"
          @click="usage.loadMoreSessions()"
        >
          {{ t('hub.st.loadMore', { n: sessions.length, total: usage.sessionsTotal.value }) }}
        </button>
      </div>
    </div>
  </div>
</template>
