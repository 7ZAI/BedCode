<script setup lang="ts">
/**
 * TaskQueueSheet — 底部抽屉传输队列 (Mobile)
 *
 * v2 四 tab（spec 14.4）：全部 | 正在发送 | 正在接收 | 历史。
 * 发送/接收以发起方区分：发送 = initiator 'me' 的任务；接收 = 宿主
 * receiving-changed 快照（只可取消，无暂停/恢复）；历史 = history-changed
 * 快照（只读 + 清空 + 打开本地文件）。
 *
 * 状态按 spec 9.3 四色体系呈现；waiting-approval（等待对方同意）计入
 * 排队视觉（琥珀 → 灰，进度条半透明）。视觉语言复用宿主 group-card /
 * group-row / status-badge / icon-chip，字号全部 clamp() 流式缩放。
 */
import { computed, ref } from 'vue'
import type { Task, ReceivingTask, HistoryEntry } from '../types'
import { TASK_STATE_KEYS, TASK_STATE_COLOR_CLASS, TASK_STATE_PROGRESS_CLASS, isTerminalState } from '../types'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'

type Translate = (key: string, params?: Record<string, any>) => string

const props = defineProps<{
  open: boolean
  tasks: Task[]
  receiving: ReceivingTask[]
  history: HistoryEntry[]
  speedMap: Record<string, number>
  totalSpeed: number
  resumableCount: number
  t: Translate
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'pause', id: string): void
  (e: 'resume', id: string): void
  (e: 'cancel', id: string): void
  (e: 'retry', id: string): void
  (e: 'remove', id: string): void
  (e: 'open', id: string): void
  (e: 'resume-all'): void
  (e: 'cancel-receiving', sessionId: string): void
  (e: 'clear-history'): void
  (e: 'open-history', entry: HistoryEntry): void
}>()

const t = props.t

/** 当前 tab（v2 四分类） */
const tab = ref<'all' | 'sending' | 'receiving' | 'history'>('all')

/** tab 定义（模板 v-for 数据源） */
const TABS: { key: 'all' | 'sending' | 'receiving' | 'history'; labelKey: string }[] = [
  { key: 'all', labelKey: 'transfer.queue.all' },
  { key: 'sending', labelKey: 'transfer.queue.sending' },
  { key: 'receiving', labelKey: 'transfer.queue.receiving' },
  { key: 'history', labelKey: 'transfer.queue.history' },
]

/** 活跃传输数（仅真正在传输的任务；排队/暂停/失败不计入，避免数字与状态不符） */
const activeCount = computed(
  () => props.tasks.filter(tk => tk.state === 'transferring').length,
)

/** 发送 tab 任务（仅本端上传；本端发起的下载归「全部」tab，避免接收方向任务混入发送语义） */
const sendingTasks = computed(() => props.tasks.filter(tk => tk.direction === 'upload'))

/** 当前 tab 展示列表（全部 = 任务全表；发送 = 仅上传；接收/历史 = 各自快照） */
const visibleTasks = computed(() => (tab.value === 'sending' ? sendingTasks.value : props.tasks))

/** 任务是否可暂停 */
function canPause(task: Task): boolean {
  return task.state === 'transferring'
}

/** 任务是否可恢复 */
function canResume(task: Task): boolean {
  return task.state === 'paused' || task.state === 'resumable'
}

/** 任务是否可取消（非终态） */
function canCancel(task: Task): boolean {
  return !isTerminalState(task.state)
}

/** 任务是否可重新排队 */
function canRetry(task: Task): boolean {
  return task.state === 'failed' || task.state === 'rejected'
}

/** 任务文件名（远端路径 basename） */
function taskName(task: Task): string {
  return task.remotePath.split('/').pop() || task.remotePath
}

/** 任务进度百分比（未知大小返回 0，不渲染数字） */
function taskPercent(task: Task): number {
  return progressPercent(task.offset, task.size) ?? 0
}

/** 任务 meta 文案：已传/总大小 + 速率（传输中） */
function taskMeta(task: Task): string {
  const size = task.size > 0
    ? `${formatBytes(task.offset, t)} / ${formatBytes(task.size, t)}`
    : formatBytes(task.offset, t)
  if (task.state === 'transferring') {
    return `${size} · ${formatSpeed(props.speedMap[task.id] ?? 0, t)}`
  }
  return size
}

/** 失败/拒绝原因文案（v2 扩展三种拒绝文案） */
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

/** 操作按钮（暂停/恢复/取消/重新排队） */
function actionButtons(task: Task): Array<{ key: string; label: string; color: string; onClick: () => void }> {
  const btns: Array<{ key: string; label: string; color: string; onClick: () => void }> = []
  if (canPause(task)) {
    btns.push({ key: 'pause', label: t('transfer.task.pause'), color: 'ft-btn-neutral', onClick: () => emit('pause', task.id) })
  }
  if (canResume(task)) {
    btns.push({ key: 'resume', label: t('transfer.task.resume'), color: 'ft-btn-accent', onClick: () => emit('resume', task.id) })
  }
  if (canRetry(task)) {
    btns.push({ key: 'retry', label: t('transfer.task.retry'), color: 'ft-btn-accent', onClick: () => emit('retry', task.id) })
  }
  if (canCancel(task)) {
    btns.push({ key: 'cancel', label: t('transfer.task.cancel'), color: 'ft-btn-neutral', onClick: () => emit('cancel', task.id) })
  }
  // 删除：任意状态可用（终态任务无生命周期动作，删除是唯一清理途径）
  btns.push({ key: 'remove', label: t('transfer.task.remove'), color: 'ft-btn-neutral', onClick: () => emit('remove', task.id) })
  // 打开本地文件：仅已完成任务（文件已落盘）
  if (task.state === 'completed') {
    btns.push({ key: 'open', label: t('transfer.task.open'), color: 'ft-btn-accent', onClick: () => emit('open', task.id) })
  }
  return btns
}

/** 接收中任务状态文案（transferring → 「正在接收」；终态 → 结果文案） */
function receivingStateKey(task: ReceivingTask): string {
  if (task.state === 'transferring') return 'transfer.task.receiving'
  switch (task.state) {
    case 'completed': return 'transfer.history.results.completed'
    case 'failed': return 'transfer.history.results.failed'
    case 'rejected': return 'transfer.history.results.rejected'
    case 'cancelled': return 'transfer.history.results.cancelled'
    default: return 'transfer.history.results.failed'
  }
}

/** 接收任务文件名（远端相对路径 basename） */
function receivingName(task: ReceivingTask): string {
  return task.remotePath.split('/').pop() || task.remotePath
}

/** 历史条目的结果文案 key（completed/failed/rejected/cancelled） */
function historyStateKey(state: string): string {
  switch (state) {
    case 'completed': return 'transfer.history.results.completed'
    case 'failed': return 'transfer.history.results.failed'
    case 'rejected': return 'transfer.history.results.rejected'
    case 'cancelled': return 'transfer.history.results.cancelled'
    default: return 'transfer.history.results.failed'
  }
}

/** 相对时间（刚刚 / N 分钟前；>60 分钟显示空串由调用方兜底） */
function relativeTime(ms: number): string | null {
  if (!ms) return null
  const mins = Math.max(0, Math.floor((Date.now() - ms) / 60000))
  if (mins < 1) return t('transfer.time.justNow')
  if (mins < 60) return t('transfer.time.minutesAgo', { count: mins })
  return null
}

/** 历史条目的时间展示（>1 小时回落绝对时间 HH:MM） */
function historyTime(ms: number): string {
  const rel = relativeTime(ms)
  if (rel) return rel
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 方向图标 path（下载 ↓ / 上传 ↑；历史条目同用） */
function directionIconPath(direction: string): string {
  return direction === 'download'
    ? 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4'
    : 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12'
}

/** 任务方向 → icon-chip 配色：统一中性灰（方向由箭头图标本身表达，
    状态色只属于 chip / 进度条，避免一卡内方向色与状态色混用） */
function directionChipClass(): string {
  return 'chip-zinc'
}
</script>

<template>
  <Teleport to="body">
    <Transition name="ft-sheet">
      <div v-if="open" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="emit('close')"></div>

        <!-- Panel -->
        <div class="ft-sheet-panel relative w-full flex flex-col bg-[var(--mobile-bg-card)] border-t border-[var(--mobile-border)] rounded-t-2xl shadow-xl">
          <!-- 抓把 -->
          <div class="flex-shrink-0 flex justify-center pt-2.5 pb-1">
            <div class="w-10 h-1 rounded-full bg-[var(--mobile-border-hover)]"></div>
          </div>

          <!-- 标题行 -->
          <div class="flex-shrink-0 flex items-center gap-2 px-4 py-2">
            <h3 class="flex-1 ft-sheet-title text-[var(--mobile-text-primary)]">
              {{ t('transfer.queue.title') }}
            </h3>
            <span v-if="activeCount > 0" class="status-badge badge-cyan">
              {{ t('transfer.queue.active', { count: activeCount }) }}
            </span>
            <button
              v-if="resumableCount > 0 && tab !== 'history'"
              class="flex-shrink-0 ft-resume-all-btn"
              @click="emit('resume-all')"
            >
              {{ t('transfer.task.resumeAll') }}
            </button>
          </div>

          <!-- v2 四 tab（全部 | 正在发送 | 正在接收 | 历史） -->
          <div class="flex-shrink-0 flex gap-1 px-4 pb-1">
            <button
              v-for="item in TABS"
              :key="item.key"
              class="ft-tab-btn flex-1"
              :class="{ 'ft-tab-btn--active': tab === item.key }"
              @click="tab = item.key"
            >
              {{ t(item.labelKey) }}
            </button>
          </div>

          <!-- 任务卡列表 -->
          <div class="flex-1 overflow-y-auto min-h-0 px-4 pb-[calc(var(--safe-area-bottom,0px)+12px)]">
            <!-- ============ 全部 / 正在发送 ============ -->
            <template v-if="tab === 'all' || tab === 'sending'">
              <div v-if="visibleTasks.length === 0" class="py-10 text-center">
                <p class="ft-task-empty">{{ t('transfer.task.empty') }}</p>
              </div>

              <div v-for="task in visibleTasks" :key="task.id" class="group-card mb-3">
                <!-- 首行：方向图标 + 名称 + 状态 chip -->
                <div class="group-row" style="gap: 0.625rem">
                  <span class="icon-chip flex-shrink-0" :class="directionChipClass()">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="directionIconPath(task.direction)" />
                    </svg>
                  </span>
                  <div class="flex-1 min-w-0">
                    <p class="ft-task-name text-[var(--mobile-text-primary)] truncate">{{ taskName(task) }}</p>
                    <p class="ft-task-meta mt-0.5 truncate">{{ taskMeta(task) }}</p>
                  </div>
                  <span
                    class="flex-shrink-0 ft-chip"
                    :class="TASK_STATE_COLOR_CLASS[task.state]"
                  >
                    {{ t(TASK_STATE_KEYS[task.state]) }}
                  </span>
                </div>

                <!-- 进度条（仅非终态） -->
                <div v-if="!isTerminalState(task.state)" class="px-4 pb-2.5">
                  <div class="ft-progress-track">
                    <div
                      class="h-full rounded-full transition-all duration-300"
                      :class="[TASK_STATE_PROGRESS_CLASS[task.state], { 'ft-progress-inactive': task.state === 'paused' || task.state === 'resumable' }]"
                      :style="{ width: taskPercent(task) + '%' }"
                    ></div>
                  </div>
                </div>

                <!-- 失败/拒绝原因：左色条 + 浅色底的内联错误块 -->
                <div v-if="taskReason(task)" class="px-4 pb-2.5">
                  <p class="ft-task-reason">{{ taskReason(task) }}</p>
                </div>

                <!-- 操作按钮 -->
                <div v-if="actionButtons(task).length > 0" class="px-4 pb-3 pt-1 flex gap-2">
                  <button
                    v-for="btn in actionButtons(task)"
                    :key="btn.key"
                    class="flex-1 ft-task-action-btn"
                    :class="btn.color"
                    @click="btn.onClick"
                  >
                    {{ btn.label }}
                  </button>
                </div>
              </div>
            </template>

            <!-- ============ 正在接收（只可取消，无暂停/恢复） ============ -->
            <template v-else-if="tab === 'receiving'">
              <div v-if="receiving.length === 0" class="py-10 text-center">
                <p class="ft-task-empty">{{ t('transfer.task.empty') }}</p>
              </div>

              <div v-for="task in receiving" :key="task.sessionId" class="group-card mb-3">
                <div class="group-row" style="gap: 0.625rem">
                  <span class="icon-chip flex-shrink-0 chip-zinc">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                    </svg>
                  </span>
                  <div class="flex-1 min-w-0">
                    <p class="ft-task-name text-[var(--mobile-text-primary)] truncate">{{ receivingName(task) }}</p>
                    <p class="ft-task-meta mt-0.5 truncate">{{ formatBytes(task.size, t) }}</p>
                  </div>
                  <span
                    class="flex-shrink-0 ft-chip"
                    :class="task.state === 'transferring' ? 'ft-color-active' : 'ft-color-completed'"
                  >
                    {{ t(receivingStateKey(task)) }}
                  </span>
                </div>

                <!-- 传输中：进度条（接收任务无偏移数据，仅展示活动态） -->
                <div v-if="task.state === 'transferring'" class="px-4 pb-2.5">
                  <div class="ft-progress-track">
                    <div class="h-full rounded-full ft-progress-active ft-progress-indeterminate"></div>
                  </div>
                </div>

                <!-- 接收任务只可取消（spec 14.3：暂停/恢复仅限发起方） -->
                <div v-if="task.state === 'transferring'" class="px-4 pb-3 pt-1 flex gap-2">
                  <button
                    class="flex-1 ft-task-action-btn ft-btn-neutral"
                    @click="emit('cancel-receiving', task.sessionId)"
                  >
                    {{ t('transfer.task.cancel') }}
                  </button>
                </div>
              </div>
            </template>

            <!-- ============ 历史（只读 + 清空 + 打开） ============ -->
            <template v-else>
              <div v-if="history.length === 0" class="py-10 text-center">
                <p class="ft-task-empty">{{ t('transfer.history.empty') }}</p>
              </div>
              <div v-else class="pb-2 pt-1 flex justify-end">
                <button class="ft-history-clear-btn" @click="emit('clear-history')">
                  {{ t('transfer.history.clear') }}
                </button>
              </div>

              <div v-for="entry in history" :key="entry.id" class="group-card mb-3">
                <div class="group-row" style="gap: 0.625rem">
                  <span class="icon-chip flex-shrink-0 chip-zinc">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="directionIconPath(entry.direction)" />
                    </svg>
                  </span>
                  <div class="flex-1 min-w-0">
                    <p class="ft-task-name text-[var(--mobile-text-primary)] truncate">{{ entry.fileName }}</p>
                    <p class="ft-task-meta mt-0.5 truncate">
                      {{ formatBytes(entry.size, t) }} · {{ historyTime(entry.updatedAt) }}
                    </p>
                  </div>
                  <span class="flex-shrink-0 ft-chip" :class="entry.state === 'completed' ? 'ft-color-completed' : entry.state === 'cancelled' ? 'ft-color-cancelled' : 'ft-color-failed'">
                    {{ t(historyStateKey(entry.state)) }}
                  </span>
                </div>

                <!-- 完成且本地有文件：打开所在文件夹（FileProvider 暴露父目录） -->
                <div v-if="entry.state === 'completed' && entry.localPath" class="px-4 pb-3 pt-1 flex gap-2">
                  <button
                    class="flex-1 ft-task-action-btn ft-btn-accent"
                    @click="emit('open-history', entry)"
                  >
                    {{ t('transfer.history.openFolder') }}
                  </button>
                </div>
              </div>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* 面板最大高度：小屏防溢出，平板展示更多任务 */
.ft-sheet-panel {
  max-height: 78dvh;
}

/* 队列标题 */
.ft-sheet-title {
  font-size: clamp(0.9375rem, 1rem + (100vw - 360px) / 800, 1.0625rem);
  font-weight: 600;
}

/* v2 tab 按钮：自绘分段（禁原生控件外观）；44px 触控目标，激活态 accent tint */
.ft-tab-btn {
  min-height: 2.5rem;
  padding: 0.375rem 0.5rem;
  border-radius: 0.625rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  color: var(--mobile-text-secondary);
  background: transparent;
  transition: background-color 0.15s ease, color 0.15s ease;
  -webkit-tap-highlight-color: transparent;
}

.ft-tab-btn:active {
  opacity: 0.8;
}

.ft-tab-btn--active {
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
}

/* 全部恢复按钮 */
.ft-resume-all-btn {
  padding: 0.25rem 0.75rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  transition: opacity 0.15s ease;
}

.ft-resume-all-btn:active {
  opacity: 0.8;
}

/* 空态文字 */
.ft-task-empty {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  color: var(--mobile-text-muted);
}

/* 任务名称 */
.ft-task-name {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  font-weight: 500;
}

/* 任务元信息（数字等宽对齐 60.0/100.0/200.0） */
.ft-task-meta {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
  font-variant-numeric: tabular-nums;
}

/* 暂停/可恢复任务的进度条：半透明降低「正在活动」的错觉，状态色仍可辨 */
.ft-progress-inactive {
  opacity: 0.45;
}

/* 接收任务不确定进度（宿主无偏移推送）：accent 底 + 呼吸动画 */
.ft-progress-indeterminate {
  animation: ft-progress-breathe 1.6s ease-in-out infinite;
}

@keyframes ft-progress-breathe {
  0%, 100% { opacity: 0.5; }
  50% { opacity: 1; }
}

/* 进度条轨道 */
.ft-progress-track {
  height: 0.375rem;
  border-radius: 9999px;
  background: var(--mobile-bg-tertiary);
  overflow: hidden;
}

/* 失败/拒绝原因：内联错误块（左色条 + 浅红底），与状态 chip 视觉呼应 */
.ft-task-reason {
  margin: 0;
  padding: 0.5rem 0.625rem;
  border-radius: 0.5rem;
  border-left: 3px solid var(--mobile-error);
  background: color-mix(in srgb, var(--mobile-error) 8%, transparent);
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  line-height: 1.45;
  color: var(--mobile-error);
}

/* 历史清空按钮：次级文字样式，44px 触控目标 */
.ft-history-clear-btn {
  min-height: 2.5rem;
  padding: 0 0.75rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-error);
  transition: opacity 0.15s ease;
}

.ft-history-clear-btn:active {
  opacity: 0.8;
}

/* 操作按钮：44px 最小触控高度 */
.ft-task-action-btn {
  min-height: 2.75rem;
  padding: 0.5rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  transition: opacity 0.15s ease;
}

.ft-task-action-btn:active {
  opacity: 0.8;
}
</style>
