<template>
  <div class="att-tab-body">
    <div
      ref="scrollEl"
      class="att-scroll"
      @touchstart.passive="onPullStart"
      @touchmove.passive="onPullMove"
      @touchend="onPullEnd"
      @touchcancel="onPullEnd"
    >
      <!-- 下拉刷新指示器（自绘状态机：idle → pulling / ready → refreshing） -->
      <div
        class="att-pull"
        :class="{ 'att-pull-anim': !pullingActive }"
        :style="{ height: pullDistance + 'px' }"
      >
        <span v-if="pullState === 'refreshing'" class="att-spinner"></span>
        <svg
          v-else
          class="att-pull-arrow"
          :class="{ 'att-pull-arrow--ready': pullState === 'ready' }"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
        </svg>
        <span class="att-pull-text">{{ pullText }}</span>
      </div>

      <!-- min-h-full + flex-col：空态/加载态在可视区内垂直居中，列表态保持顶部对齐 -->
      <div class="att-scroll-inner">
        <!-- 状态筛选 chips -->
        <div class="att-chips">
          <button
            v-for="chip in chips"
            :key="chip.value"
            class="att-chip"
            :class="{ 'att-chip-active': statusFilter === chip.value }"
            @click="props.history.setStatusFilter(chip.value)"
          >
            {{ t(chip.label) }}
          </button>
        </div>

        <!-- 未连接空态 -->
        <div v-if="offline && tasks.length === 0" class="att-state">
          <svg class="att-state-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M18.364 5.636a9 9 0 010 12.728m-12.728 0a9 9 0 010-12.728m9.9 2.828a5 5 0 010 7.072m-7.072 0a5 5 0 010-7.072M12 12h.01" />
          </svg>
          <p>{{ t('history.offline') }}</p>
        </div>

        <!-- 首屏加载 -->
        <div v-else-if="loading && tasks.length === 0" class="att-state">
          <p>{{ t('history.loading') }}</p>
        </div>

        <!-- 空态 -->
        <div v-else-if="tasks.length === 0" class="att-state">
          <svg class="att-state-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
          </svg>
          <p>{{ t('history.empty') }}</p>
        </div>

        <!-- 任务列表 -->
        <div v-else class="att-list">
          <div v-for="task in tasks" :key="task.id" class="att-row">
            <div class="att-row-head">
              <p class="att-row-title">{{ task.description || t('history.noDescription') }}</p>
              <span class="att-badge" :style="badgeStyle(task.status)">{{ statusLabel(task.status) }}</span>
            </div>
            <div class="att-row-meta">
              <span v-if="task.agent" class="att-meta-item">{{ t('history.field.agent') }}: {{ task.agent }}</span>
              <span class="att-meta-item">{{ t('history.field.source') }}: {{ sourceLabel(task.source) }}</span>
              <span class="att-meta-item">{{ t('history.field.time') }}: {{ utcToLocalDisplay(task.created_at) }}</span>
              <span v-if="task.started_at && task.completed_at" class="att-meta-item">
                {{ t('history.field.duration') }}: {{ formatDuration(task.started_at, task.completed_at) }}
              </span>
            </div>
          </div>
        </div>

        <!-- 加载更多 / 没有更多 -->
        <div v-if="tasks.length > 0" class="att-footer">
          <button
            v-if="hasMore"
            class="att-load-more"
            :disabled="loadingMore"
            @click="props.history.loadMore()"
          >
            {{ loadingMore ? t('history.loading') : t('history.loadMore') }}
          </button>
          <span v-else class="att-no-more">{{ t('history.noMore') }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * TaskHistoryTab — 任务记录页（纯 UI）
 *
 * 数据与交互逻辑在 useTaskHistory composable（宿主容器传入实例）：
 * 状态筛选 chips / 分页加载更多 / 下拉刷新均只调用其暴露的 action。
 */
import { ref, computed } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import type { TaskHistoryComposable, HistoryStatusFilter } from '../composables/useTaskHistory'
import { utcToLocalDisplay, formatDuration } from '../composables/useTaskHistory'

const props = defineProps<{
  context: PluginContext
  history: TaskHistoryComposable
}>()

const t = (key: string): string => props.context.i18n.t(key)

// 解构 ref：模板顶层自动解包（composable 实例经 props 传入时模板不会自动解包）
const { tasks, hasMore, statusFilter, loading, loadingMore, offline } = props.history

// ==================== 状态筛选 chips ====================

const chips: { value: HistoryStatusFilter; label: string }[] = [
  { value: 'all', label: 'history.statusFilter.all' },
  { value: 'in_progress', label: 'history.statusFilter.in_progress' },
  { value: 'completed', label: 'history.statusFilter.completed' },
  { value: 'interrupted', label: 'history.statusFilter.interrupted' },
  { value: 'failed', label: 'history.statusFilter.failed' },
]

// ==================== 状态/来源展示 ====================

/** 状态徽标文字：五筛选项走页签 i18n，其余（idle/asking/pending/waiting）走既有平铺 key */
function statusLabel(status: string): string {
  switch (status) {
    case 'in_progress':
      return t('history.statusFilter.in_progress')
    case 'completed':
      return t('history.statusFilter.completed')
    case 'interrupted':
      return t('history.statusFilter.interrupted')
    case 'failed':
      return t('history.statusFilter.failed')
    case 'asking':
      return t('asking')
    case 'pending':
      return t('pending')
    case 'waiting':
      return t('waiting')
    default:
      return t('idle')
  }
}

/** 状态徽标底色：状态色 10% tint + 状态色文字（与 atp-clear-btn 同款扁平风格） */
function badgeStyle(status: string): Record<string, string> {
  const color: Record<string, string> = {
    idle: 'var(--mobile-text-disabled)',
    in_progress: 'var(--mobile-accent)',
    asking: '#f59e0b',
    completed: '#22c55e',
    interrupted: 'var(--mobile-error)',
    failed: 'var(--mobile-error)',
    pending: 'var(--mobile-text-disabled)',
    waiting: '#f59e0b',
  }
  const c = color[status] || color.idle
  return {
    background: `color-mix(in srgb, ${c} 10%, transparent)`,
    color: c,
  }
}

function sourceLabel(source: string | null): string {
  switch (source) {
    case 'user':
      return t('history.source.user')
    case 'queue':
      return t('history.source.queue')
    case 'preset':
      return t('history.source.preset')
    default:
      return t('history.source.unknown')
  }
}

// ==================== 下拉刷新（自绘状态机，与 FileTransferView 一致） ====================

const PULL_TRIGGER = 56
const PULL_MAX = 96
const PULL_RESISTANCE = 0.45

const scrollEl = ref<HTMLElement | null>(null)
const pullDistance = ref(0)
const pullState = ref<'idle' | 'pulling' | 'ready' | 'refreshing'>('idle')
const pullingActive = ref(false)
let pullStartY = 0

const pullText = computed(() => {
  if (pullState.value === 'refreshing') return t('common.refreshing')
  if (pullState.value === 'ready') return t('common.releaseRefresh')
  return t('common.pullRefresh')
})

function onPullStart(e: TouchEvent): void {
  const el = scrollEl.value
  if (!el || el.scrollTop > 0 || loading.value || pullState.value === 'refreshing') return
  pullingActive.value = true
  pullStartY = e.touches[0].clientY
}

function onPullMove(e: TouchEvent): void {
  if (!pullingActive.value) return
  const dy = e.touches[0].clientY - pullStartY
  if (dy <= 0) {
    if (pullDistance.value !== 0) {
      pullDistance.value = 0
      pullState.value = 'idle'
    }
    return
  }
  pullDistance.value = Math.min(dy * PULL_RESISTANCE, PULL_MAX)
  pullState.value = pullDistance.value >= PULL_TRIGGER ? 'ready' : 'pulling'
}

function onPullEnd(): void {
  if (!pullingActive.value) return
  pullingActive.value = false
  if (pullState.value === 'ready') {
    // 释放刷新：指示器常驻刷新态，刷新完成后回弹（保留当前筛选，回到第一页）
    pullState.value = 'refreshing'
    pullDistance.value = PULL_TRIGGER
    void props.history.refresh().finally(() => {
      pullState.value = 'idle'
      pullDistance.value = 0
    })
  } else {
    pullState.value = 'idle'
    pullDistance.value = 0
  }
}
</script>
