<script setup lang="ts">
/**
 * TaskPanel — 传输队列面板（右侧滑入）— host-peer 契约版
 *
 * 四 tab：全部 | 正在发送 | 正在接收 | 历史。
 * 任务为批级记录（一批 = 一条）：传输中可取消，失败/拒绝可重试；
 * 接收 tab 仅可取消；历史只读 + 清空。状态 chips（四色体系）+ 进度条 + 速率。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { HistoryEntry, ReceivingTask, Task, TaskStateName } from '../types'
import { formatBytes, formatEta, displayName, formatClock } from '../utils/format'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const props = defineProps<{
  tasks: Task[]
  /** 传输中任务总速率 */
  totalSpeed: number
  /** 接收中任务（正在接收 tab） */
  receiving: ReceivingTask[]
  /** 传输历史（历史 tab） */
  history: HistoryEntry[]
  /** 对端名映射（peerId → 展示名） */
  peerNames: Record<string, string>
  /** 本端下载目录（下载完成「打开所在目录」定位用） */
  downloadDir: string
}>()

const emit = defineEmits<{
  (e: 'cancel', id: string): void
  (e: 'retry', id: string): void
  (e: 'cancelReceiving', sessionId: string): void
  (e: 'clearHistory'): void
  (e: 'openFolder', path: string): void
}>()

/** 队列 tab（自绘分段控件，禁原生 select） */
type QueueTab = 'all' | 'sending' | 'receiving' | 'history'
const activeTab = ref<QueueTab>('all')

/** 状态 → 展示文案 key */
const STATE_KEYS: Record<TaskStateName, string> = {
  transferring: 'transfer.task.state.transferring',
  completed: 'transfer.task.state.completed',
  failed: 'transfer.task.state.failed',
  rejected: 'transfer.task.state.rejected',
  cancelled: 'transfer.task.state.cancelled',
  interrupted: 'transfer.task.state.interrupted',
}

/** 状态 → chip 样式（四色体系） */
const CHIP_CLASS: Record<TaskStateName, string> = {
  transferring: 'ft-chip--active',
  completed: 'ft-chip--active',
  failed: 'ft-chip--fail',
  rejected: 'ft-chip--reject',
  cancelled: 'ft-chip--queued',
  interrupted: 'ft-chip--queued',
}

function stateLabel(state: TaskStateName): string {
  return t(STATE_KEYS[state])
}

function chipClass(state: TaskStateName): string {
  return CHIP_CLASS[state] ?? 'ft-chip--queued'
}

function stateText(state: TaskStateName): string {
  return stateLabel(state)
}

/** 进度百分比 */
function percent(task: Task): number {
  if (task.size <= 0) return task.state === 'completed' ? 100 : 0
  return Math.min(100, Math.round((task.offset / task.size) * 100))
}

function canRetry(task: Task): boolean {
  // interrupted（插件重启恢复标注）与 failed/rejected 同样可重试
  return task.state === 'failed' || task.state === 'rejected' || task.state === 'interrupted'
}
function canCancel(task: Task): boolean {
  return !isTerminal(task.state)
}

function isTerminal(state: TaskStateName): boolean {
  return (
    state === 'completed' ||
    state === 'failed' ||
    state === 'rejected' ||
    state === 'cancelled' ||
    state === 'interrupted'
  )
}

function speedOf(task: Task): number {
  return task.rateBps ?? 0
}

function etaOf(task: Task): string {
  const sp = speedOf(task)
  if (sp <= 0 || task.size <= 0 || task.state !== 'transferring') return ''
  return formatEta((task.size - task.offset) / sp, t)
}

/** 拒绝原因映射 */
function rejectReasonText(reason: string | null | undefined): string {
  switch (reason) {
    case 'user-rejected':
      return t('transfer.error.rejectedByUser')
    case 'timeout':
      return t('transfer.error.noResponse')
    case 'policy-denied':
      return t('transfer.error.policyDenied')
    case 'duplicate-name':
      return t('transfer.error.duplicateName')
    default:
      return ''
  }
}

/** 失败/拒绝原因文案 */
function reasonText(task: Task): string {
  if (task.state === 'rejected')
    return rejectReasonText(task.reason) || t('transfer.task.state.rejected')
  if (task.state === 'failed') return rejectReasonText(task.reason) || (task.reason ?? '')
  return ''
}

/** 对端展示名（peerId → 缓存名 → 原始 ID） */
function peerNameOf(peerId: string): string {
  return props.peerNames[peerId] || peerId || '—'
}

/** 接收任务对端展示名 */
function receivingPeerName(task: ReceivingTask | undefined): string {
  if (!task) return ''
  const name = task.peerName as string | undefined
  if (name) return name
  return task.peerId ? peerNameOf(task.peerId) : ''
}

/** 接收任务状态归一化：running/transferring → transferring，其余终态原样 */
function receivingStateName(r: ReceivingTask | undefined): TaskStateName {
  const s = r?.state
  if (
    s === 'completed' ||
    s === 'failed' ||
    s === 'rejected' ||
    s === 'cancelled' ||
    s === 'interrupted'
  ) {
    return s
  }
  return 'transferring'
}

/** 接收任务进度百分比 */
function receivingPercent(r: ReceivingTask | undefined): number {
  if (!r) return 0
  if (r.size <= 0) return r.state === 'completed' ? 100 : 0
  return Math.min(100, Math.round(((r.offset ?? 0) / r.size) * 100))
}

/** 接收任务是否展示进度条（传输中或已完成） */
function receivingHasProgress(r: ReceivingTask | undefined): boolean {
  return !!r && (r.state === 'transferring' || r.state === 'running' || r.state === 'completed')
}

/** 按 sessionId 取接收任务（全部 tab 混排时用） */
function receivingById(id: string): ReceivingTask | undefined {
  return props.receiving.find((r) => r.sessionId === id)
}

/** 本地落盘路径：下载目录 + 首文件相对路径（reveal 定位用；缺相对路径退回下载目录） */
function localPathOf(relPath: string | null | undefined): string | null {
  if (!props.downloadDir) return null
  if (!relPath) return props.downloadDir
  const base = props.downloadDir.replace(/[\\/]+$/, '')
  return `${base}/${relPath.replace(/^[\\/]+/, '')}`
}

/** 下载完成 → 打开本地所在目录（无落盘目录时静默忽略） */
function openFolderAt(relPath: string | null | undefined): void {
  const p = localPathOf(relPath)
  if (p) emit('openFolder', p)
}

/** tab 列表：正在发送 = 本端上传；全部 = 本端任务 + 接收任务合并倒序 */
const tabItems = computed(() => {
  if (activeTab.value === 'sending') {
    return props.tasks
      .filter((tk) => tk.direction === 'upload')
      .slice()
      .sort((a, b) => b.createdAt - a.createdAt)
      .map((tk) => ({ id: tk.id, kind: 'task' as const }))
  }
  const items: Array<{ id: string; kind: 'task' | 'receiving'; createdAt: number }> = [
    ...props.tasks.map((tk) => ({ id: tk.id, kind: 'task' as const, createdAt: tk.createdAt })),
    ...props.receiving.map((r) => ({
      id: r.sessionId,
      kind: 'receiving' as const,
      createdAt: r.createdAt,
    })),
  ]
  items.sort((a, b) => b.createdAt - a.createdAt)
  return items
})

/** 历史结果文案 */
function historyResult(entry: HistoryEntry): string {
  return t(`transfer.history.results.${entry.state}`)
}

/** 历史条目原因文案（仅失败/拒绝时显示） */
function historyReason(entry: HistoryEntry): string {
  if (entry.state === 'failed' && entry.reason)
    return rejectReasonText(entry.reason) || entry.reason
  if (entry.state === 'rejected')
    return rejectReasonText(entry.reason) || t('transfer.task.state.rejected')
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

      <!-- 总速率（仅传输中显示） -->
      <div v-if="totalSpeed > 0" class="ft-summary-speed">
        {{ t('transfer.summary.speed', { speed: formatBytes(totalSpeed) }) }}
      </div>

      <!-- 队列 4 tab（自绘分段，禁原生 select） -->
      <div class="ft-tabs" role="tablist">
        <button
          v-for="tab in ['all', 'sending', 'receiving', 'history'] as QueueTab[]"
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
            <span
              class="ft-task-dir"
              :class="entry.direction === 'upload' ? 'ft-task-dir--up' : ''"
            >
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
                <span class="ft-task-name" :title="entry.fileName">{{
                  displayName(entry.fileName)
                }}</span>
                <span class="ft-chip" :class="chipClass(entry.state)">{{
                  historyResult(entry)
                }}</span>
                <!-- 下载完成且落盘：打开本地所在目录 -->
                <button
                  v-if="entry.direction === 'download' && entry.state === 'completed'"
                  class="ft-mini-btn"
                  :title="t('transfer.history.openFolder')"
                  @click="openFolderAt(entry.relPath)"
                >
                  <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                    />
                  </svg>
                </button>
              </div>
              <div class="ft-history-meta">
                <span>{{ formatClock(entry.updatedAt) }}</span>
                <span>{{ formatBytes(entry.size) }}</span>
                <span v-if="entry.peerName">{{ entry.peerName }}</span>
              </div>
              <div v-if="historyReason(entry)" class="ft-task-reason">
                {{ historyReason(entry) }}
              </div>
            </div>
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
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M12 19V5M5 12l7-7 7 7"
                  />
                </svg>
              </span>
              <span class="ft-task-name" :title="r.remotePath">{{
                displayName(r.remotePath)
              }}</span>
              <span class="ft-chip" :class="chipClass(receivingStateName(r))">{{
                stateLabel(receivingStateName(r))
              }}</span>
              <!-- 下载完成：打开本地所在目录 -->
              <button
                v-if="r.state === 'completed'"
                class="ft-mini-btn"
                :title="t('transfer.task.openDir')"
                @click="openFolderAt(r.relPath)"
              >
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                  />
                </svg>
              </button>
              <!-- 接收任务只可取消（spec §14.3：暂停/恢复仅限发起方） -->
              <button
                class="ft-mini-btn"
                :title="t('transfer.task.cancel')"
                @click="emit('cancelReceiving', r.sessionId)"
              >
                <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M18 6L6 18M6 6l12 12"
                  />
                </svg>
              </button>
            </div>
            <div v-if="receivingHasProgress(r)" class="ft-pbar">
              <span class="ft-pbar-fill" :style="{ width: receivingPercent(r) + '%' }"></span>
            </div>
            <div class="ft-task-meta">
              <span>{{ formatBytes(r.offset ?? 0) }} / {{ formatBytes(r.size) }}</span>
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
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M12 19V5M5 12l7-7 7 7"
                    />
                  </svg>
                </span>
                <span class="ft-task-name" :title="receivingById(item.id)?.remotePath ?? ''">
                  {{ displayName(receivingById(item.id)?.remotePath ?? '') }}
                </span>
                <span
                  class="ft-chip"
                  :class="chipClass(receivingStateName(receivingById(item.id)))"
                >
                  {{ stateLabel(receivingStateName(receivingById(item.id))) }}
                </span>
                <!-- 下载完成：打开本地所在目录 -->
                <button
                  v-if="receivingById(item.id)?.state === 'completed'"
                  class="ft-mini-btn"
                  :title="t('transfer.task.openDir')"
                  @click="openFolderAt(receivingById(item.id)?.relPath)"
                >
                  <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                    />
                  </svg>
                </button>
                <button
                  class="ft-mini-btn"
                  :title="t('transfer.task.cancel')"
                  @click="emit('cancelReceiving', item.id)"
                >
                  <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M18 6L6 18M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>
              <div v-if="receivingHasProgress(receivingById(item.id))" class="ft-pbar">
                <span
                  class="ft-pbar-fill"
                  :style="{ width: receivingPercent(receivingById(item.id)) + '%' }"
                ></span>
              </div>
              <div class="ft-task-meta">
                <span>
                  {{ formatBytes(receivingById(item.id)?.offset ?? 0) }} /
                  {{ formatBytes(receivingById(item.id)?.size ?? 0) }}
                </span>
                <span v-if="receivingPeerName(receivingById(item.id))">{{
                  receivingPeerName(receivingById(item.id))
                }}</span>
              </div>
            </div>

            <!-- 本端任务卡 -->
            <div v-else-if="tasks.find((tk) => tk.id === item.id)" class="ft-task">
              <template v-for="task in tasks.filter((tk) => tk.id === item.id)" :key="task.id">
                <div class="ft-task-head">
                  <span
                    class="ft-task-dir"
                    :class="task.direction === 'upload' ? 'ft-task-dir--up' : ''"
                  >
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
                  <span class="ft-task-name" :title="task.remotePath">{{
                    displayName(task.remotePath)
                  }}</span>
                  <span class="ft-chip" :class="chipClass(task.state)">{{
                    stateText(task.state)
                  }}</span>
                  <button
                    v-if="canRetry(task)"
                    class="ft-mini-btn"
                    :title="t('transfer.task.retry')"
                    @click="emit('retry', task.id)"
                  >
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M1 4v6h6M3.51 15a9 9 0 102.13-9.36L1 10"
                      />
                    </svg>
                  </button>
                  <button
                    v-if="canCancel(task)"
                    class="ft-mini-btn"
                    :title="t('transfer.task.cancel')"
                    @click="emit('cancel', task.id)"
                  >
                    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M18 6L6 18M6 6l12 12"
                      />
                    </svg>
                  </button>
                </div>

                <!-- 进度条 -->
                <div class="ft-pbar">
                  <span
                    class="ft-pbar-fill"
                    :style="{ width: percent(task) + '%' }"
                  ></span>
                </div>

                <!-- 元信息 / 原因 -->
                <div class="ft-task-meta">
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
