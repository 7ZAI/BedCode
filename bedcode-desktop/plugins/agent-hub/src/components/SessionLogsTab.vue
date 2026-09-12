<script setup lang="ts">
/**
 * 会话日志分区（票据 06）—— 主从布局
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` 变体 B「会话日志」
 * 页（左 268px 会话列表 + 右侧头卡（适配器/源路径/模型 token 明细）+ 归一
 * 事件流（用户/助手/工具/系统四类角色 tag）+ 「原始 JSONL」行切换）。
 * 列表与统计明细共用 useUsage 分页数据；打开会话经 read-usage-session
 * 按需解析（与统计共用同一适配器层，单会话一次读盘双消费）。
 */
import { computed, inject, ref, watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { NormalizedEventView } from '../types'
import type { UseUsageReturn } from '../composables/useUsage'
import {
  abbreviateProject,
  formatDuration,
  formatEventTime,
  formatSessionTime,
  formatTokens,
} from '../utils/format'

const props = defineProps<{ usage: UseUsageReturn }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const usage = props.usage
const state = computed(() => usage.state.value)
const home = computed(() => state.value?.home ?? '')
const authRequired = computed(() => state.value?.status === 'auth-required')
const syncing = computed(() => state.value?.status === 'syncing')
const sessions = computed(() => usage.sessions.value)

/** 当前打开会话（日志详情）；切换过滤后若会话不在新列表则保留展示 */
const opened = computed(() => usage.openedSession.value)

/** 原始 JSONL 行视图开关 */
const showRaw = ref(false)
watch(opened, () => {
  showRaw.value = false
})

const FILTERS = ['', 'claude', 'pi'] as const

/** 事件角色 → tag 类名 */
function roleClass(role: NormalizedEventView['role']): string {
  switch (role) {
    case 'user':
      return 'user'
    case 'assistant':
      return 'asst'
    case 'tool':
      return 'tool'
    default:
      return 'sys'
  }
}

/** 助手事件 token 明细（↑ 输入 ↓ 输出 ⚡ 缓存读 +缓存写） */
function tokenMeta(e: NormalizedEventView): string {
  if (!e.tokens) return ''
  return [
    `↑ ${formatTokens(e.tokens.input)}`,
    `↓ ${formatTokens(e.tokens.output)}`,
    `⚡ ${formatTokens(e.tokens.cacheRead)}`,
    `+${formatTokens(e.tokens.cacheWrite)}`,
    e.tokens.reasoning > 0 ? `◈ ${formatTokens(e.tokens.reasoning)}` : '',
  ]
    .filter(Boolean)
    .join(' · ')
}

const hasMore = computed(() => sessions.value.length < usage.sessionsTotal.value)
</script>

<template>
  <div class="ah-lg">
    <div class="ah-lg-list">
      <div class="ah-lg-filters">
        <button
          v-for="f in FILTERS"
          :key="f"
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm"
          :class="{ 'ah-btn-primary': usage.listFilter.value === f }"
          @click="usage.setListFilter(f)"
        >
          {{ f === '' ? t('hub.lg.filterAll') : f }}
        </button>
      </div>
      <div v-if="sessions.length === 0" class="ah-st-empty">
        {{ authRequired ? t('hub.auth.banner') : t('hub.st.empty') }}
      </div>
      <div
        v-for="s in sessions"
        :key="`${s.adapter}-${s.id}`"
        class="ah-card ah-lg-item"
        :class="{ active: opened?.session.id === s.id }"
        role="button"
        tabindex="0"
        @click="usage.openSession(s.id)"
        @keydown.enter="usage.openSession(s.id)"
      >
        <div class="ah-lg-item-top">
          <span class="ah-cli-tag ok">{{ s.adapter }}</span>
          <span class="ah-lg-item-title">{{ s.title || s.cli_session_id }}</span>
        </div>
        <div class="ah-lg-item-sub ah-mono">
          {{ formatSessionTime(s.started_at) }} · {{ formatDuration(s.duration_ms) }} ·
          {{ formatTokens((s.tokens_in || 0) + (s.tokens_out || 0)) }}
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

    <div class="ah-lg-view">
      <div v-if="!opened" class="ah-card ah-lg-placeholder">
        {{ syncing ? t('hub.st.scanning') : t('hub.lg.pickSession') }}
      </div>
      <template v-else>
        <div class="ah-card ah-lg-head">
          <div class="ah-lg-head-row">
            <span class="ah-cli-tag ok">{{ opened.session.adapter }}</span>
            <span class="ah-lg-head-title">{{ opened.session.title || opened.session.cli_session_id }}</span>
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-raw-toggle"
              :class="{ 'ah-btn-primary': showRaw }"
              @click="showRaw = !showRaw"
            >
              {{ showRaw ? t('hub.lg.viewEvents') : t('hub.lg.viewRaw') }}
            </button>
          </div>
          <div class="ah-lg-head-src ah-mono" :title="opened.session.source_path || ''">
            {{ opened.session.source_path || t('hub.lg.noSource') }}
          </div>
          <div class="ah-lg-head-meta ah-mono">
            <span v-if="opened.session.model">{{ opened.session.model }}</span>
            <span>↑ {{ formatTokens(opened.session.tokens_in) }} ↓ {{ formatTokens(opened.session.tokens_out) }}</span>
            <span>⚡ {{ formatTokens(opened.session.tokens_cache_read) }} +{{ formatTokens(opened.session.tokens_cache_write) }}</span>
            <span v-if="opened.session.tokens_reasoning > 0">◈ {{ formatTokens(opened.session.tokens_reasoning) }}</span>
            <span>{{ abbreviateProject(opened.session.project, home) }}</span>
          </div>
          <div v-if="opened.eventsTruncated" class="ah-lg-warn">{{ t('hub.lg.eventsTruncated') }}</div>
          <div v-if="opened.rawTruncated" class="ah-lg-warn">{{ t('hub.lg.rawTruncated') }}</div>
        </div>

        <div v-if="showRaw" class="ah-card">
          <div class="ah-lg-raw ah-mono">
            <div v-for="(line, i) in opened.raw" :key="i" class="ah-lg-raw-line">{{ line }}</div>
          </div>
        </div>

        <div v-else class="ah-card">
          <div v-if="opened.events.length === 0" class="ah-st-empty">{{ t('hub.lg.noEvents') }}</div>
          <div v-for="(e, i) in opened.events" :key="i" class="ah-lg-evt">
            <span class="ah-lg-evt-t ah-mono">{{ formatEventTime(e.ts) }}</span>
            <span class="ah-lg-tag" :class="roleClass(e.role)">{{ t(`hub.lg.role.${e.role}`) }}</span>
            <div class="ah-lg-evt-tx">
              <div class="ah-lg-evt-text">{{ e.text }}</div>
              <div v-if="e.tokens" class="ah-lg-evt-meta ah-mono">
                <span v-if="e.model">{{ e.model }}</span>
                <span>{{ tokenMeta(e) }}</span>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>
