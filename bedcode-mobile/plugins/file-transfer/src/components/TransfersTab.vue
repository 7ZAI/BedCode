<script setup lang="ts">
/**
 * TransfersTab — 传输列表 tab
 *
 * 与旧版的核心差异：传输列表**直接铺在主内容区**（不再收进 bottom sheet），
 * 用四个筛选 chip 划分 全部 / 发送 / 接收 / 历史。「全部」为发送队列 ∪ 接收中
 * （计数与展示同口径，历史归档单独成 tab）。领域模型由本层映射为
 * TaskCard 的纯展示 props（文案格式化在此完成，卡内不含逻辑）。
 *
 * 状态呈现仍走 spec 9.3 四色体系（ft-color-* / ft-progress-*）。
 */
import { computed, ref } from 'vue'
import type { Task, ReceivingTask, HistoryEntry } from '../types'
import {
  TASK_STATE_KEYS,
  TASK_STATE_COLOR_CLASS,
  TASK_STATE_PROGRESS_CLASS,
  isTerminalState,
} from '../types'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'
import type { TaskAction, TransferFilter } from '../types'
import TaskCard from './TaskCard.vue'
import EmptyState from './EmptyState.vue'

type Translate = (key: string, params?: Record<string, any>) => string

const props = defineProps<{
  tasks: Task[]
  receiving: ReceivingTask[]
  history: HistoryEntry[]
  t: Translate
}>()

const emit = defineEmits<{
  (e: 'cancel', id: string): void
  (e: 'retry', id: string): void
  (e: 'cancel-receiving', sessionId: string): void
  (e: 'clear-history'): void
  (e: 'open-location', id: string): void
}>()

const t = props.t

/** 当前筛选 */
const filter = ref<TransferFilter>('all')

/** 显示名：远端相对路径取 basename */
function basename(path: string): string {
  return path.split('/').pop() || path
}

/** 发送 tab：仅本端发出的批 */
const sendingTasks = computed(() => props.tasks.filter((tk) => tk.direction === 'upload'))

/** 各筛选的条目数（chip 角标；全部 = 发送队列 ∪ 接收中，历史归档不计入） */
const counts = computed(() => ({
  all: props.tasks.length + props.receiving.length,
  sending: sendingTasks.value.length,
  receiving: props.receiving.length,
  history: props.history.length,
}))

/**
 * 展示集合（与 chip 计数同口径）：
 * 「全部」渲染发送队列卡 + 接收卡；「发送/接收」单筛各自只取自己的集合。
 */
const visibleQueue = computed(() => {
  if (filter.value === 'sending') return sendingTasks.value
  if (filter.value === 'receiving') return []
  return props.tasks
})

const receivingCards = computed(() =>
  filter.value === 'receiving' || filter.value === 'all' ? props.receiving : [],
)

/** 活动传输区是否整体为空（决定空态显隐） */
const listEmpty = computed(() => visibleQueue.value.length === 0 && receivingCards.value.length === 0)

const FILTERS = computed<Array<{ key: TransferFilter; labelKey: string; count: number }>>(() => [
  { key: 'all', labelKey: 'transfer.v2.filter.all', count: counts.value.all },
  { key: 'sending', labelKey: 'transfer.v2.filter.sending', count: counts.value.sending },
  { key: 'receiving', labelKey: 'transfer.v2.filter.receiving', count: counts.value.receiving },
  { key: 'history', labelKey: 'transfer.v2.filter.history', count: counts.value.history },
])

/** 空态图标（按当前筛选给不同语义图标） */
const emptyIcon = computed(() => {
  switch (filter.value) {
    case 'receiving':
      return 'M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-5.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293H4'
    case 'history':
      return 'M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z'
    default:
      return 'M13 10V3L4 14h7v7l9-11h-7z'
  }
})

/** 空态文案 */
const emptyTitle = computed(() => {
  if (filter.value === 'history') return t('transfer.history.empty')
  if (filter.value === 'receiving') return t('transfer.task.empty')
  return t('transfer.task.empty')
})

// ==================== 发送 / 全部（Task → 卡片） ====================

/** 任务元信息：已传/总大小 + 速率（仅传输中带速率） */
function taskMeta(task: Task): string {
  const size = task.size > 0
    ? `${formatBytes(task.offset, t)} / ${formatBytes(task.size, t)}`
    : formatBytes(task.offset, t)
  return task.state === 'transferring' ? `${size} · ${formatSpeed(task.rateBps ?? 0, t)}` : size
}

/** 失败/拒绝原因文案（reason wire → 展示文案 key） */
function taskReason(task: Task): string | null {
  if (task.state !== 'failed' && task.state !== 'rejected') return null
  switch (task.reason) {
    case 'duplicate-name':
    case 'DuplicateName':
      return t('transfer.task.reason.duplicateName')
    case 'remote-changed':
      return t('transfer.task.reason.remoteChanged')
    case 'no-roots':
      return t('transfer.task.reason.noRoots')
    case 'local file not found':
      return t('transfer.task.reason.localNotFound')
    case 'user-rejected':
      return t('transfer.error.rejectedByUser')
    case 'timeout':
      return t('transfer.error.noResponse')
    case 'policy-denied':
      return t('transfer.error.policyDenied')
    default:
      return task.reason ? String(task.reason) : t('transfer.task.reason.unknown')
  }
}

/** 任务操作：终态不可取消；failed/rejected/interrupted 可重试 */
function taskActions(task: Task): TaskAction[] {
  const btns: TaskAction[] = []
  if (task.state === 'failed' || task.state === 'rejected' || task.state === 'interrupted') {
    btns.push({ kind: 'retry', label: t('transfer.task.retry'), variant: 'tint' })
  }
  if (!isTerminalState(task.state)) {
    btns.push({ kind: 'cancel', label: t('transfer.task.cancel'), variant: 'neutral' })
  }
  return btns
}

// ==================== 接收中（ReceivingTask → 卡片） ====================

/** 接收中状态文案（running/transferring → 正在接收；终态 → 结果文案） */
function receivingStateKey(task: ReceivingTask): string {
  if (task.state === 'running' || task.state === 'transferring') return 'transfer.task.receiving'
  switch (task.state) {
    case 'completed': return 'transfer.history.results.completed'
    case 'failed': return 'transfer.history.results.failed'
    case 'rejected': return 'transfer.history.results.rejected'
    case 'cancelled': return 'transfer.history.results.cancelled'
    default: return 'transfer.history.results.failed'
  }
}

function receivingStateClass(task: ReceivingTask): string {
  if (task.state === 'running' || task.state === 'transferring') return 'ft-color-active'
  if (task.state === 'completed') return 'ft-color-completed'
  if (task.state === 'cancelled') return 'ft-color-cancelled'
  return 'ft-color-failed'
}

const RECEIVING_TERMINAL = new Set(['completed', 'failed', 'rejected', 'cancelled'])

// ==================== 历史（HistoryEntry → 卡片） ====================

/** 相对时间：刚刚 / N 分钟前 / N 小时前；>24 小时回退 MM-DD（历史跨天不写年份） */
function historyTime(ms: number): string {
  if (!ms) return ''
  const mins = Math.max(0, Math.floor((Date.now() - ms) / 60000))
  if (mins < 1) return t('transfer.time.justNow')
  if (mins < 60) return t('transfer.time.minutesAgo', { count: mins })
  if (mins < 1440) return t('transfer.time.hoursAgo', { count: Math.floor(mins / 60) })
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

function historyStateKey(state: string): string {
  switch (state) {
    case 'completed': return 'transfer.history.results.completed'
    case 'failed': return 'transfer.history.results.failed'
    case 'rejected': return 'transfer.history.results.rejected'
    case 'cancelled': return 'transfer.history.results.cancelled'
    default: return 'transfer.history.results.failed'
  }
}

function historyStateClass(state: string): string {
  if (state === 'completed') return 'ft-color-completed'
  if (state === 'cancelled') return 'ft-color-cancelled'
  return 'ft-color-failed'
}

/** 取消原因码 → i18n（兼容旧引擎本地化文本 wire：'cancelled by sender' 等） */
function cancelledReason(reason: string): string {
  switch (reason) {
    case 'cancelled-by-sender':
    case 'cancelled by sender':
      return t('transfer.task.reason.cancelledBySender')
    case 'cancelled-by-receiver':
    case 'cancelled by receiver':
      return t('transfer.task.reason.cancelledByReceiver')
    case 'cancelled-by-self':
    case 'cancelled by self':
      return t('transfer.task.reason.cancelledBySelf')
    default:
      return reason
  }
}

/** 接收卡原因文案（wire detail → i18n；未知回退原文） */
function receivingReason(task: ReceivingTask): string | null {
  if (!task.reason) return null
  return cancelledReason(String(task.reason))
}

/** 历史条目原因文案（取消码 i18n 化；failed detail 保留原文） */
function historyReason(entry: HistoryEntry): string | null {
  if (!entry.reason) return null
  return cancelledReason(String(entry.reason))
}

function historyMeta(entry: HistoryEntry): string {
  const time = historyTime(entry.updatedAt)
  return time ? `${formatBytes(entry.size, t)} · ${time}` : formatBytes(entry.size, t)
}

/** 历史条目操作：下载完成 → 打开所在位置；发起方失败/被拒/中断且可重试 → 重试
 *（终态归档历史后，重试入口从发送列表迁到历史） */
function historyActions(entry: HistoryEntry): TaskAction[] {
  const btns: TaskAction[] = []
  if (entry.direction === 'download' && entry.state === 'completed') {
    btns.push({ kind: 'open-location', label: t('transfer.v2.history.openFolder'), variant: 'tint' })
  }
  if (
    entry.retryable &&
    (entry.state === 'failed' || entry.state === 'rejected' || entry.state === 'interrupted')
  ) {
    btns.push({ kind: 'retry', label: t('transfer.task.retry'), variant: 'tint' })
  }
  return btns
}

/** 操作分发：kind → 对应 emit */
function onCardAction(kind: TaskAction['kind'], id: string): void {
  switch (kind) {
    case 'cancel': emit('cancel', id); break
    case 'retry': emit('retry', id); break
    case 'cancel-receiving': emit('cancel-receiving', id); break
    case 'clear-history': emit('clear-history'); break
    case 'open-location': emit('open-location', id); break
  }
}
</script>

<template>
  <div class="flex-1 min-h-0 flex flex-col">
    <!-- 筛选 chip 行（横向滚动，计数 tabular-nums） -->
    <div class="fv2-filters">
      <button
        v-for="f in FILTERS"
        :key="f.key"
        class="fv2-filter"
        :class="{ 'fv2-filter--active': filter === f.key }"
        @click="filter = f.key"
      >
        {{ t(f.labelKey) }}
        <span v-if="f.count > 0" class="fv2-filter-count">{{ f.count }}</span>
      </button>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto overscroll-behavior-none px-4 pb-2">
      <!-- 全部 / 发送 / 接收：活动传输区（全部 = 发送队列 ∪ 接收中） -->
      <template v-if="filter !== 'history'">
        <div v-if="listEmpty" class="h-full flex flex-col">
          <EmptyState :icon="emptyIcon" :title="emptyTitle" :hint="t('transfer.minibar.noActive')" />
        </div>
        <template v-else>
          <TaskCard
            v-for="task in visibleQueue"
            :key="task.id"
            :id="task.id"
            :direction="task.direction"
            :name="basename(task.remotePath)"
            :meta="taskMeta(task)"
            :state-label="t(TASK_STATE_KEYS[task.state])"
            :state-class="TASK_STATE_COLOR_CLASS[task.state]"
            :progress="isTerminalState(task.state) ? null : (progressPercent(task.offset, task.size) ?? 0)"
            :progress-class="TASK_STATE_PROGRESS_CLASS[task.state]"
            :reason="taskReason(task)"
            :actions="taskActions(task)"
            @action="onCardAction"
          />
          <TaskCard
            v-for="task in receivingCards"
            :key="task.sessionId"
            :id="task.sessionId"
            direction="download"
            :name="basename(task.remotePath)"
            :meta="task.size > 0 ? `${formatBytes(task.offset ?? 0, t)} / ${formatBytes(task.size, t)}` : formatBytes(task.size, t)"
            :state-label="t(receivingStateKey(task))"
            :state-class="receivingStateClass(task)"
            :progress="RECEIVING_TERMINAL.has(task.state) ? null : (progressPercent(task.offset ?? 0, task.size) ?? null)"
            progress-class="ft-progress-active"
            :indeterminate="(task.state === 'running' || task.state === 'transferring') && (task.offset ?? 0) === 0"
            :reason="receivingReason(task)"
            :actions="task.state === 'running' || task.state === 'transferring'
              ? [{ kind: 'cancel-receiving', label: t('transfer.task.cancel'), variant: 'neutral' }]
              : []"
            @action="onCardAction"
          />
        </template>
      </template>

      <!-- 历史：只读 + 清空 -->
      <template v-else>
        <div v-if="history.length === 0" class="h-full flex flex-col">
          <EmptyState :icon="emptyIcon" :title="emptyTitle" />
        </div>
        <template v-else>
          <div class="flex justify-end pb-1">
            <button class="fv2-btn-text" @click="emit('clear-history')">
              <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
              {{ t('transfer.history.clear') }}
            </button>
          </div>
          <TaskCard
            v-for="entry in history"
            :key="entry.id"
            :id="entry.id"
            :direction="entry.direction"
            :name="entry.fileName"
            :meta="historyMeta(entry)"
            :state-label="t(historyStateKey(entry.state))"
            :state-class="historyStateClass(entry.state)"
            :progress="null"
            :reason="historyReason(entry)"
            :actions="historyActions(entry)"
            @action="onCardAction"
          />
        </template>
      </template>
    </div>
  </div>
</template>
