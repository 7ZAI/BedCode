<script setup lang="ts">
/**
 * 会话日志分区（票 06 + 改版）—— 查询表格 + 二级详情
 *
 * 一级页：日志来源（折叠区：内置/自定义目录 + 扫描 + 添加目录）→ 多条件查询
 * （Agent / 关键词 / 时间范围）→ 分页表格（点击行进二级详情）。
 * 二级页：会话详情头卡 + 页签切换（聊天记录 / 原始 JSONL），聊天为只读
 * 对话样式（类 zcode/trae，无输入框）。
 *
 * 数据流：列表分页与统计明细共用 useUsage 同一查询域；打开会话经
 * read-usage-session 按需解析（单会话一次读盘双消费）。
 */
import { computed, inject, ref, watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'
import type { NormalizedEventView, UsageSource } from '../types'
import type { UseUsageReturn } from '../composables/useUsage'
import {
  abbreviateProject,
  formatDuration,
  formatEventTime,
  formatSessionTime,
  formatTokens,
} from '../utils/format'
import AgentIcon from './AgentIcon.vue'

const props = defineProps<{ usage: UseUsageReturn }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const usage = props.usage
const state = computed(() => usage.state.value)
const home = computed(() => state.value?.home ?? '')
const authRequired = computed(() => state.value?.status === 'auth-required')
const syncing = computed(() => state.value?.status === 'syncing')
const sessions = computed(() => usage.sessions.value)
const sources = computed(() => usage.sources.value)
const sessionsTotal = computed(() => usage.sessionsTotal.value)
const totalPages = computed(() => usage.totalPages.value)
const page = computed(() => usage.page.value)
const loadingSessions = computed(() => usage.loadingSessions.value)

/** 当前打开会话（二级详情）；打开会话重置页签 */
const opened = computed(() => usage.openedSession.value)
const detailTab = ref<'chat' | 'raw'>('chat')
watch(opened, () => {
  detailTab.value = 'chat'
})

// ==================== 来源区（默认折叠） ====================
const sourcesOpen = ref(false)
const addingSource = ref(false)
const newSourceName = ref('')
const newSourcePath = ref('')
const sourceBusy = ref(false)
const sourceError = ref('')
const removingSource = ref('')

/** 来源扫描计数摘要（未扫描过显示提示） */
function scanCount(s: UsageSource): string {
  const sc = s.scan
  if (!sc) return t('hub.lg.sources.noScan')
  return `${sc.parsed + sc.skipped} files · ${sc.sessions} ${t('hub.lg.sources.sessions')}`
}

async function onAddSource() {
  if (!newSourceName.value.trim() || !newSourcePath.value.trim()) return
  sourceBusy.value = true
  sourceError.value = ''
  const r = await usage.addSource(newSourceName.value.trim(), newSourcePath.value.trim())
  sourceBusy.value = false
  if (r.ok) {
    addingSource.value = false
    newSourceName.value = ''
    newSourcePath.value = ''
  } else {
    sourceError.value = r.error ?? t('hub.lg.sources.addFailed')
  }
}

function cancelAddSource() {
  addingSource.value = false
  newSourceName.value = ''
  newSourcePath.value = ''
  sourceError.value = ''
}

async function onRemoveSource(name: string) {
  removingSource.value = name
  sourceError.value = ''
  const r = await usage.removeSource(name)
  removingSource.value = ''
  if (!r.ok) sourceError.value = r.error ?? t('hub.lg.sources.removeFailed')
}

// ==================== 查询条件（Agent / 关键词 / 时间范围） ====================
const filterAgent = ref(usage.listFilter.value)
const keyword = ref(usage.searchText.value)
const fromInput = ref('')
const toInput = ref('')

/** Agent 下拉选项（SDK Select，与宿主表单控件同源） */
const agentOptions = computed(() => [
  { value: '', label: t('hub.lg.filter.all') },
  ...sources.value.map((s) => ({ value: s.name, label: s.name })),
])

/** 执行查询：提交条件 → 回第 1 页 */
async function applyQuery() {
  usage.listFilter.value = filterAgent.value
  usage.searchText.value = keyword.value
  usage.rangeFrom.value = fromInput.value ? new Date(fromInput.value).getTime() : null
  usage.rangeTo.value = toInput.value ? new Date(toInput.value).getTime() : null
  await usage.reloadSessions()
}

/** 重置全部条件 → 回第 1 页 */
async function applyReset() {
  filterAgent.value = ''
  keyword.value = ''
  fromInput.value = ''
  toInput.value = ''
  await usage.resetQuery()
}

function openDetail(id: number) {
  void usage.openSession(id)
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
</script>

<template>
  <div class="ah-lg">
    <!-- ==================== 一级：来源区 + 查询 + 分页表格 ==================== -->
    <template v-if="!opened">
      <!-- 日志来源（默认折叠；内置只读 + 自定义增删 + 扫描） -->
      <div class="ah-lg-sources">
        <button
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-sources-toggle"
          :aria-expanded="sourcesOpen"
          @click="sourcesOpen = !sourcesOpen"
        >
          <span class="ah-lg-sources-chev" :class="{ open: sourcesOpen }">▸</span>
          {{ t('hub.lg.sources.title') }}
          <span class="ah-lg-sources-count">{{ sources.length }}</span>
        </button>
        <div v-if="sourcesOpen" class="ah-card ah-lg-sources-body">
          <div v-for="s in sources" :key="s.name" class="ah-lg-source-row">
            <AgentIcon :adapter="s.name" :size="16" />
            <span class="ah-lg-source-name">{{ s.name }}</span>
            <span class="ah-lg-source-type" :class="s.builtin ? 'builtin' : 'custom'">
              {{ s.builtin ? t('hub.lg.sources.builtin') : t('hub.lg.sources.custom') }}
            </span>
            <span class="ah-lg-source-path ah-mono" :title="s.path">{{ abbreviateProject(s.path, home) }}</span>
            <span class="ah-lg-source-scan ah-mono">{{ scanCount(s) }}</span>
            <button
              v-if="!s.builtin"
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-source-remove"
              :disabled="removingSource === s.name"
              :title="t('hub.lg.sources.remove')"
              @click="onRemoveSource(s.name)"
            >
              ✕
            </button>
          </div>
          <div class="ah-lg-sources-actions">
            <button
              type="button"
              class="ah-btn ah-btn-primary ah-btn-sm"
              :disabled="syncing"
              @click="usage.scan()"
            >
              {{ syncing ? t('hub.st.scanning') : t('hub.lg.sources.scan') }}
            </button>
            <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" @click="addingSource = true">
              {{ t('hub.lg.sources.add') }}
            </button>
            <span v-if="sourceError" class="ah-cli-error">{{ sourceError }}</span>
          </div>
          <div v-if="addingSource" class="ah-lg-sources-add">
            <input
              v-model="newSourceName"
              class="h-[var(--input-height)] min-w-[180px] flex-1 rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] px-4 text-[var(--text-primary)] outline-none transition-all duration-200 shadow-xs placeholder:text-[var(--text-tertiary)] focus:border-brand focus:shadow-input-focus dark:shadow-none"
              :placeholder="t('hub.lg.sources.addName')"
              :disabled="sourceBusy"
            />
            <input
              v-model="newSourcePath"
              class="ah-mono h-[var(--input-height)] min-w-[180px] flex-1 rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] px-4 text-[var(--text-primary)] outline-none transition-all duration-200 shadow-xs placeholder:text-[var(--text-tertiary)] focus:border-brand focus:shadow-input-focus dark:shadow-none"
              :placeholder="t('hub.lg.sources.addPath')"
              :disabled="sourceBusy"
            />
            <span class="ah-speed-actions-btns">
              <button
                type="button"
                class="ah-btn ah-btn-primary ah-btn-sm"
                :disabled="sourceBusy || !newSourceName.trim() || !newSourcePath.trim()"
                @click="onAddSource"
              >
                {{ t('hub.lg.sources.confirm') }}
              </button>
              <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="sourceBusy" @click="cancelAddSource">
                {{ t('hub.lg.sources.cancel') }}
              </button>
            </span>
          </div>
        </div>
      </div>

      <!-- 多条件查询条 -->
      <div class="ah-card ah-lg-filter">
        <label class="ah-lg-filter-field">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.agent') }}</span>
          <Select v-model="filterAgent" class="min-w-[120px]" :options="agentOptions" />
        </label>
        <label class="ah-lg-filter-field ah-lg-filter-q">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.keyword') }}</span>
          <input
            v-model="keyword"
            class="w-full h-[var(--input-height)] rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] px-4 text-[var(--text-primary)] outline-none transition-all duration-200 shadow-xs placeholder:text-[var(--text-tertiary)] focus:border-brand focus:shadow-input-focus dark:shadow-none"
            :placeholder="t('hub.lg.filter.keywordPh')"
            @keydown.enter="applyQuery"
          />
        </label>
        <label class="ah-lg-filter-field">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.from') }}</span>
          <input
            v-model="fromInput"
            type="datetime-local"
            class="w-full h-[var(--input-height)] rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] px-4 text-[var(--text-primary)] outline-none transition-all duration-200 shadow-xs focus:border-brand focus:shadow-input-focus dark:shadow-none"
          />
        </label>
        <label class="ah-lg-filter-field">
          <span class="ah-lg-filter-label">{{ t('hub.lg.filter.to') }}</span>
          <input
            v-model="toInput"
            type="datetime-local"
            class="w-full h-[var(--input-height)] rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] px-4 text-[var(--text-primary)] outline-none transition-all duration-200 shadow-xs focus:border-brand focus:shadow-input-focus dark:shadow-none"
          />
        </label>
        <div class="ah-lg-filter-actions">
          <button type="button" class="ah-btn ah-btn-primary ah-btn-sm" :disabled="loadingSessions" @click="applyQuery">
            {{ t('hub.lg.filter.query') }}
          </button>
          <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="loadingSessions" @click="applyReset">
            {{ t('hub.lg.filter.reset') }}
          </button>
        </div>
      </div>

      <!-- 分页表格：Agent | 会话 | 项目 | 开始时间 | 时长 | Tokens -->
      <div class="ah-card ah-lg-table">
        <div class="ah-lg-table-head">
          <span class="ah-lg-col-agent">{{ t('hub.lg.col.agent') }}</span>
          <span class="ah-lg-col-title">{{ t('hub.lg.col.session') }}</span>
          <span class="ah-lg-col-project">{{ t('hub.lg.col.project') }}</span>
          <span class="ah-lg-col-time">{{ t('hub.lg.col.started') }}</span>
          <span class="ah-lg-col-dur">{{ t('hub.lg.col.duration') }}</span>
          <span class="ah-lg-col-tokens">{{ t('hub.lg.col.tokens') }}</span>
          <span class="ah-lg-col-go"></span>
        </div>

        <div class="ah-lg-table-rows">
          <div v-if="sessions.length === 0 && !loadingSessions" class="ah-st-empty">
            {{ authRequired ? t('hub.auth.banner') : t('hub.lg.noMatch') }}
          </div>
          <div
            v-for="s in sessions"
            :key="s.id"
            class="ah-lg-row"
            :class="{ 'ah-lg-row-active': s.active }"
            role="button"
            tabindex="0"
            :title="s.active ? t('hub.lg.row.currentTip') : t('hub.lg.row.open')"
            @click="openDetail(s.id)"
            @keydown.enter="openDetail(s.id)"
          >
          <div class="ah-lg-col-agent ah-lg-cell-agent">
            <AgentIcon :adapter="s.adapter" :size="16" />
            <span class="ah-lg-cell-agent-name">{{ s.adapter }}</span>
            <span v-if="s.active" class="ah-lg-badge-current">{{ t('hub.lg.row.current') }}</span>
          </div>
          <div class="ah-lg-col-title ah-lg-cell-title" :title="s.title || s.cli_session_id">
            {{ s.title || s.cli_session_id }}
          </div>
          <div class="ah-lg-col-project ah-lg-cell-sub" :title="s.project || ''">
            {{ abbreviateProject(s.project, home) }}
          </div>
          <div class="ah-lg-col-time ah-mono">{{ formatSessionTime(s.started_at) }}</div>
          <div class="ah-lg-col-dur ah-mono">{{ formatDuration(s.duration_ms) }}</div>
          <div class="ah-lg-col-tokens ah-mono">
            {{ formatTokens((s.tokens_in || 0) + (s.tokens_out || 0)) }}
          </div>
          <div class="ah-lg-col-go">›</div>
        </div>
        </div>

        <div class="ah-lg-pager">
          <span class="ah-lg-pager-info">
            {{ t('hub.lg.pager.total', { total: sessionsTotal }) }} ·
            {{ t('hub.lg.pager.page', { page, pages: totalPages }) }}
          </span>
          <span class="ah-speed-actions-btns">
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :disabled="page <= 1 || loadingSessions"
              @click="usage.goPage(page - 1)"
            >
              ‹ {{ t('hub.lg.pager.prev') }}
            </button>
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :disabled="page >= totalPages || loadingSessions"
              @click="usage.goPage(page + 1)"
            >
              {{ t('hub.lg.pager.next') }} ›
            </button>
          </span>
        </div>
      </div>
    </template>

    <!-- ==================== 二级：日志详情（页签：聊天 / 原始 JSONL） ==================== -->
    <template v-else>
      <div class="ah-lg-detail">
        <div class="ah-card ah-lg-detail-head">
          <div class="ah-lg-detail-head-row">
            <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm ah-lg-detail-back" @click="usage.closeSession()">
              ← {{ t('hub.lg.detail.back') }}
            </button>
            <AgentIcon :adapter="opened.session.adapter" :size="16" />
            <span class="ah-lg-detail-title">{{ opened.session.title || opened.session.cli_session_id }}</span>
            <span class="ah-cli-tag ok">{{ opened.session.adapter }}</span>
            <span class="ah-lg-detail-tabs" role="tablist">
              <button
                type="button"
                class="ah-lg-tab"
                :class="{ active: detailTab === 'chat' }"
                role="tab"
                :aria-selected="detailTab === 'chat'"
                @click="detailTab = 'chat'"
              >
                {{ t('hub.lg.detail.tabChat') }}
              </button>
              <button
                type="button"
                class="ah-lg-tab"
                :class="{ active: detailTab === 'raw' }"
                role="tab"
                :aria-selected="detailTab === 'raw'"
                @click="detailTab = 'raw'"
              >
                {{ t('hub.lg.detail.tabRaw') }}
              </button>
            </span>
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

        <!-- 聊天记录（只读对话样式，无输入框） -->
        <div v-if="detailTab === 'chat'" class="ah-card ah-lg-events">
          <div v-if="opened.events.length === 0" class="ah-st-empty">{{ t('hub.lg.noEvents') }}</div>
          <div v-else class="ah-chat">
            <div class="ah-chat-scroll">
              <div
                v-for="(e, i) in opened.events"
                :key="i"
                class="ah-msg"
                :class="`role-${e.role}`"
              >
                <div class="ah-msg-head">
                  <span class="ah-msg-dot" aria-hidden="true"></span>
                  <span class="ah-msg-role">{{ t(`hub.lg.role.${e.role}`) }}</span>
                  <span v-if="e.model" class="ah-msg-model ah-mono">{{ e.model }}</span>
                  <span class="ah-msg-time ah-mono">{{ formatEventTime(e.ts) }}</span>
                </div>
                <div class="ah-msg-body">
                  <div class="ah-msg-text">{{ e.text }}</div>
                  <div v-if="e.tokens" class="ah-msg-meta ah-mono">
                    <span>{{ tokenMeta(e) }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 原始 JSONL 行视图 -->
        <div v-else class="ah-card ah-lg-raw-card">
          <div class="ah-lg-raw ah-mono">
            <div v-for="(line, i) in opened.raw" :key="i" class="ah-lg-raw-line">{{ line }}</div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>