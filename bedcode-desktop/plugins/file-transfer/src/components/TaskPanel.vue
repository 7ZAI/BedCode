<script setup lang="ts">
/**
 * TaskPanel — 传输队列面板（右侧 360px 常驻）
 *
 * v2 四 tab：全部 | 正在发送 | 正在接收 | 历史（spec §14.4）。
 * - 全部 = 本端任务（非终态）+ 接收任务合并，按创建时间倒序
 * - 正在发送 = 本端上传任务（waiting-approval 显示「等待对方同意」+ 可取消）
 * - 正在接收 = 对端发起的接收任务（只可取消，无暂停/恢复）
 * - 历史 = 终态归档只读条目（时间/方向/文件名/大小/结果 + 清空 + 打开所在文件夹）
 * 状态汇总 chips（四色体系，spec §9.3）+ 进度条 + 速率 · 剩余时间。
 * 纯展示组件，动作经 emit 交给父级 composable。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { HistoryEntry, ReceivingTask, Task, TaskStateName } from '../types'
import { TASK_STATE_KEYS } from '../composables/useTasks'
import { formatBytes, formatEta, displayName, formatClock } from '../utils/format'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const props = defineProps<{
  tasks: Task[]
  /** 逐任务速率（字节/秒，来自快照差分） */
  speedMap: Record<string, number>
  summary: { active: number; queued: number; failed: number; rejected: number; resumable: number; paused: number }
  /** 可恢复任务数（resume-all 按钮条件） */
  resumableCount: number
  /** 传输中任务总速率 */
  totalSpeed: number
  /** v2：接收中任务（正在接收 tab） */
  receiving: ReceivingTask[]
  /** v2：传输历史（历史 tab） */
  history: HistoryEntry[]
  /** v2：对端名映射（peerId → 展示名，批卡/接收任务用） */
  peerNames: Record<string, string>
}>()

const emit = defineEmits<{
  (e: 'pause', id: string): void
  (e: 'resume', id: string): void
  (e: 'cancel', id: string): void
  (e: 'retry', id: string): void
  (e: 'remove', id: string): void
  (e: 'openDir', id: string): void
  (e: 'resumeAll'): void
  (e: 'cancelReceiving', sessionId: string): void
  (e: 'clearHistory'): void
  (e: 'openHistoryDir', localPath: string): void
}>()

/** 队列 tab（自绘分段控件，禁原生 select） */
type QueueTab = 'all' | 'sending' | 'receiving' | 'history'
const activeTab = ref<QueueTab>('all')

/** 状态 → chip 样式（四色体系） */
const CHIP_CLASS: Record<TaskStateName, string> = {
  queued: 'ft-chip--queued',
  'waiting-approval': 'ft-chip--queued',
  transferring: 'ft-chip--active',
  paused: 'ft-chip--pause',
  resumable: 'ft-chip--pause',
  completed: 'ft-chip--active',
  failed: 'ft-chip--fail',
  rejected: 'ft-chip--reject',
  cancelled: 'ft-chip--queued',
}

function stateLabel(state: TaskStateName): string {
  return t(TASK_STATE_KEYS[state])
}

function chipClass(state: TaskStateName): string {
  return CHIP_CLASS[state] ?? 'ft-chip--queued'
}

/** 是否暂停类状态（进度条/动作按钮按琥珀色呈现） */
function isPausedState(state: TaskStateName): boolean {
  return state === 'paused' || state === 'resumable'
}

/** 进度百分比 */
function percent(task: Task): number {
  if (task.size <= 0) return task.state === 'completed' ? 100 : 0
  return Math.min(100, Math.round((task.offset / task.size) * 100))
}

/** 各状态可用的动作 */
function canPause(task: Task): boolean {
  return task.state === 'transferring'
}
function canResume(task: Task): boolean {
  return task.state === 'paused' || task.state === 'resumable'
}
function canRetry(task: Task): boolean {
  return task.state === 'failed' || task.state === 'rejected'
}
function canCancel(task: Task): boolean {
  return task.state !== 'completed' && task.state !== 'cancelled'
}

function speedOf(task: Task): number {
  return props.speedMap[task.id] ?? 0
}

function etaOf(task: Task): string {
  const sp = speedOf(task)
  if (sp <= 0 || task.size <= 0 || task.state !== 'transferring') return ''
  return formatEta((task.size - task.offset) / sp, t)
}

/** 已完成/进行中/暂停类展示传输元信息；终态展示原因文案 */
function showMeta(task: Task): boolean {
  return (
    task.state === 'transferring' ||
    task.state === 'paused' ||
    task.state === 'resumable' ||
    task.state === 'completed'
  )
}

/** 拒绝原因映射（v2：user-rejected / timeout / policy-denied 三文案） */
function rejectReasonText(reason: string | null | undefined): string {
  switch (reason) {
    case 'user-rejected': return t('transfer.error.rejectedByUser')
    case 'timeout': return t('transfer.error.noResponse')
    case 'policy-denied': return t('transfer.error.policyDenied')
    case 'duplicate-name': return t('transfer.error.duplicateName')
    default: return ''
  }
}

/** 失败/拒绝原因文案（复用 spec §10 + v2 错误 key） */
function reasonText(task: Task): string {
  if (task.state === 'rejected') return rejectReasonText(task.reason) || t('transfer.task.state.rejected')
  if (task.state === 'failed' && task.reason === 'duplicate-name') {
    return t('transfer.error.duplicateName')
  }
  if (task.state === 'failed' && task.reason === 'remote-changed') {
    return t('transfer.error.remoteChanged')
  }
  return task.state === 'failed' && task.reason ? task.reason : ''
}

/** 对端展示名（peerId → 缓存名 → 原始 ID） */
function peerNameOf(peerId: string): string {
  return props.peerNames[peerId] || peerId || '—'
}

/** 接收任务对端展示名（优先任务侧缓存，回退映射表） */
function receivingPeerName(task: ReceivingTask): string {
  return task.peerId ? peerNameOf(task.peerId) : ''
}

/** tab 列表：正在发送 = 本端上传任务（本端发起的下载归「全部」tab，避免
 * 接收方向任务混入发送语义）；全部 tab = 本端非终态任务 + 接收任务合并 */
const tabItems = computed(() => {
  if (activeTab.value === 'sending') {
    return props.tasks
      .filter(t => t.direction === 'upload')
      .slice()
      .sort((a, b) => b.createdAt - a.createdAt)
      .map(t => ({ id: t.id, kind: 'task' as const }))
  }
  const items: Array<{ id: string; kind: 'task' | 'receiving'; createdAt: number }> = [
    ...props.tasks.map(t => ({ id: t.id, kind: 'task' as const, createdAt: t.createdAt })),
    ...props.receiving.map(r => ({
      id: r.sessionId,
      kind: 'receiving' as const,
      createdAt: r.createdAt,
    })),
  ]
  items.sort((a, b) => b.createdAt - a.createdAt)
  return items
})

/** 历史结果文案（history.results.*） */
function historyResult(entry: HistoryEntry): string {
  return t(`transfer.history.results.${entry.state}`)
}

/** 历史条目原因文案（仅失败/拒绝时显示） */
function historyReason(entry: HistoryEntry): string {
  if (entry.state === 'failed' && entry.reason) return rejectReasonText(entry.reason) || entry.reason
  if (entry.state === 'rejected') return rejectReasonText(entry.reason) || t('transfer.task.state.rejected')
  return ''
}
</script>

<template>
  <div class="ft-queue">
    <div class="ft-queue-body">
      <!-- 面板头：传输队列 + 任务总数 -->
      <div class="ft-queue-head">
        <span class="ft-queue-title">{{ t('transfer.queue.title') }}</span>
        <span
          v-if="tasks.length > 0"
          class="ft-queue-count"
          :title="t('transfer.queue.count', { count: tasks.length })"
        >
          {{ tasks.length }}
        </span>
      </div>

      <!-- 状态汇总 chips（历史 tab 也常驻，队列概况） -->
      <div class="ft-chips">
        <span v-if="summary.active > 0" class="ft-chip ft-chip--active">
          {{ t('transfer.summary.active', { count: summary.active }) }}
        </span>
        <span v-if="summary.queued > 0" class="ft-chip ft-chip--queued">
          {{ t('transfer.summary.queued', { count: summary.queued }) }}
        </span>
        <span v-if="summary.failed > 0" class="ft-chip ft-chip--fail">
          {{ t('transfer.summary.failed', { count: summary.failed }) }}
        </span>
        <span v-if="summary.rejected > 0" class="ft-chip ft-chip--reject">
          {{ t('transfer.summary.rejected', { count: summary.rejected }) }}
        </span>
        <button
          v-if="resumableCount > 0"
          class="ft-btn ft-resume-all"
          @click="emit('resumeAll')"
        >
          {{ t('transfer.task.resumeAll') }}
        </button>
      </div>

      <!-- 总速率（仅传输中显示） -->
      <div v-if="summary.active > 0 && totalSpeed > 0" class="ft-summary-speed">
        {{ t('transfer.summary.speed', { speed: formatBytes(totalSpeed) }) }}
      </div>

      <!-- 队列 4 tab（自绘分段，禁原生 select） -->
      <div class="ft-tabs" role="tablist">
        <button
          v-for="tab in (['all', 'sending', 'receiving', 'history'] as QueueTab[])"
          :key="tab"
          class="ft-tab"
          :class="{ 'ft-tab--active': activeTab === tab }"
          role="tab"
          :aria-selected="activeTab === tab"
          @click="activeTab = tab"
        >
          {{ t(`transfer.queue.${tab}`) }}
        </button>
      </div>

      <!-- ==================== 历史 tab（只读） ==================== -->
      <div v-if="activeTab === 'history'" class="ft-history">
        <div v-if="history.length === 0" class="ft-empty">
          {{ t('transfer.history.empty') }}
        </div>
        <TransitionGroup v-else tag="div" name="ft-task" class="ft-task-list">
          <div v-for="entry in history" :key="entry.id" class="ft-history-item">
            <span class="ft-task-dir" :class="entry.direction === 'upload' ? 'ft-task-dir--up' : ''">
              <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  v-if="entry.direction === 'download'"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 5v14M19 12l-7 7-7-7"
                />
                <path
                  v-else
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 19V5M5 12l7-7 7 7"
                />
              </svg>
            </span>
            <div class="ft-history-main">
              <div class="ft-history-line">
                <span class="ft-task-name" :title="entry.fileName">{{ displayName(entry.fileName) }}</span>
                <span class="ft-chip" :class="chipClass(entry.state)">{{ historyResult(entry) }}</span>
              </div>
              <div class="ft-history-meta">
                <span>{{ formatClock(entry.updatedAt) }}</span>
                <span>{{ formatBytes(entry.size) }}</span>
                <span v-if="entry.peerName">{{ entry.peerName }}</span>
              </div>
              <div v-if="historyReason(entry)" class="ft-task-reason">{{ historyReason(entry) }}</div>
            </div>
            <!-- 打开所在文件夹：仅完成且有本地文件 -->
            <button
              v-if="entry.state === 'completed' && entry.localPath"
              class="ft-mini-btn"
              :title="t('transfer.history.openFolder')"
              @click="emit('openHistoryDir', entry.localPath)"
            >
              <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71" /></svg>
            </button>
          </div>
        </TransitionGroup>
        <button
          v-if="history.length > 0"
          class="ft-btn ft-history-clear"
          @click="emit('clearHistory')"
        >
          {{ t('transfer.history.clear') }}
        </button>
      </div>

      <!-- ==================== 正在接收 tab（只可取消） ==================== -->
      <div v-else-if="activeTab === 'receiving'" class="ft-receiving">
        <div v-if="receiving.length === 0" class="ft-empty">
          {{ t('transfer.task.receivingEmpty') }}
        </div>
        <TransitionGroup v-else tag="div" name="ft-task" class="ft-task-list">
          <div v-for="r in receiving" :key="r.sessionId" class="ft-task">
            <div class="ft-task-head">
              <span class="ft-task-dir ft-task-dir--up">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19V5M5 12l7-7 7 7" />
                </svg>
              </span>
              <span class="ft-task-name" :title="r.remotePath">{{ displayName(r.remotePath) }}</span>
              <span class="ft-chip" :class="chipClass(r.state)">{{ stateLabel(r.state === 'transferring' ? 'transferring' : r.state) }}</span>
              <!-- 接收任务只可取消（spec §14.3：暂停/恢复仅限发起方） -->
              <button class="ft-mini-btn" :title="t('transfer.task.cancel')" @click="emit('cancelReceiving', r.sessionId)">
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 6L6 18M6 6l12 12" /></svg>
              </button>
            </div>
            <div class="ft-task-meta">
              <span>{{ t('transfer.task.receiving') }}</span>
              <span>{{ formatBytes(r.size) }}</span>
              <span v-if="receivingPeerName(r)">{{ receivingPeerName(r) }}</span>
            </div>
          </div>
        </TransitionGroup>
      </div>

      <!-- ==================== 全部 / 正在发送 tab（本端任务） ==================== -->
      <div v-else>
        <!-- 空队列 -->
        <div v-if="tabItems.length === 0" class="ft-empty">
          {{ t('transfer.task.empty') }}
        </div>

        <TransitionGroup v-else tag="div" name="ft-task" class="ft-task-list">
          <template v-for="item in tabItems" :key="item.id">
            <!-- 接收任务（全部 tab 混排） -->
            <div v-if="item.kind === 'receiving'" class="ft-task">
              <div class="ft-task-head">
                <span class="ft-task-dir ft-task-dir--up">
                  <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19V5M5 12l7-7 7 7" />
                  </svg>
                </span>
                <span class="ft-task-name" :title="receiving.find(r => r.sessionId === item.id)?.remotePath ?? ''">
                  {{ displayName(receiving.find(r => r.sessionId === item.id)?.remotePath ?? '') }}
                </span>
                <span class="ft-chip ft-chip--active">{{ t('transfer.task.receiving') }}</span>
                <button class="ft-mini-btn" :title="t('transfer.task.cancel')" @click="emit('cancelReceiving', item.id)">
                  <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 6L6 18M6 6l12 12" /></svg>
                </button>
              </div>
              <div class="ft-task-meta">
                <span>{{ formatBytes(receiving.find(r => r.sessionId === item.id)?.size ?? 0) }}</span>
              </div>
            </div>

            <!-- 本端任务卡 -->
            <div v-else-if="tasks.find(tk => tk.id === item.id)" class="ft-task">
              <template v-for="task in tasks.filter(tk => tk.id === item.id)" :key="task.id">
                <div class="ft-task-head">
                  <span class="ft-task-dir" :class="task.direction === 'upload' ? 'ft-task-dir--up' : ''">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path
                        v-if="task.direction === 'download'"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M12 5v14M19 12l-7 7-7-7"
                      />
                      <path
                        v-else
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M12 19V5M5 12l7-7 7 7"
                      />
                    </svg>
                  </span>
                  <span class="ft-task-name" :title="task.remotePath">{{ displayName(task.remotePath) }}</span>
                  <span class="ft-chip" :class="chipClass(task.state)">{{ stateLabel(task.state) }}</span>
                  <button v-if="canPause(task)" class="ft-mini-btn" :title="t('transfer.task.pause')" @click="emit('pause', task.id)">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-width="2" d="M9 4h2v16H9zM15 4h2v16h-2z" /></svg>
                  </button>
                  <button v-if="canResume(task)" class="ft-mini-btn" :title="t('transfer.task.resume')" @click="emit('resume', task.id)">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 4l13 8-13 8V4z" /></svg>
                  </button>
                  <button v-if="canRetry(task)" class="ft-mini-btn" :title="t('transfer.task.retry')" @click="emit('retry', task.id)">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M1 4v6h6M3.51 15a9 9 0 102.13-9.36L1 10" /></svg>
                  </button>
                  <button v-if="canCancel(task)" class="ft-mini-btn" :title="t('transfer.task.cancel')" @click="emit('cancel', task.id)">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 6L6 18M6 6l12 12" /></svg>
                  </button>
                  <button class="ft-mini-btn" :title="t('transfer.task.remove')" @click="emit('remove', task.id)">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                  </button>
                  <!-- 打开本地目录：仅已完成任务（文件已落盘） -->
                  <button v-if="task.state === 'completed'" class="ft-mini-btn" :title="t('transfer.task.openDir')" @click="emit('openDir', task.id)">
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71" /></svg>
                  </button>
                </div>

                <!-- 进度条（终态无进度条时也渲染完成态） -->
                <div class="ft-pbar">
                  <span
                    class="ft-pbar-fill"
                    :class="{ 'ft-pbar-fill--pause': isPausedState(task.state) }"
                    :style="{ width: percent(task) + '%' }"
                  ></span>
                </div>

                <!-- 元信息 / 原因 -->
                <div v-if="showMeta(task)" class="ft-task-meta">
                  <span>{{ formatBytes(task.offset) }} / {{ formatBytes(task.size) }}</span>
                  <span v-if="speedOf(task) > 0">{{ formatBytes(speedOf(task)) }}/s</span>
                  <span v-if="etaOf(task)">{{ etaOf(task) }}</span>
                </div>
                <div v-if="reasonText(task)" class="ft-task-reason">{{ reasonText(task) }}</div>
              </template>
            </div>
          </template>
        </TransitionGroup>
      </div>
    </div>
  </div>
</template>
