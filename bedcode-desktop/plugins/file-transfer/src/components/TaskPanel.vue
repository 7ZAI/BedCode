<script setup lang="ts">
/**
 * TaskPanel — 传输队列面板（右侧 360px 常驻）
 *
 * 状态汇总 chips（四色体系，spec §9.3）+ 任务卡（方向箭头 + 文件名 +
 * 暂停/恢复/取消/重试 + 进度条 + 已完成/总量 · 速率 · 剩余时间）。
 * 纯展示组件，动作经 emit 交给父级 composable。
 */
import { inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import type { Task, TaskStateName } from '../types'
import { TASK_STATE_KEYS } from '../composables/useTasks'
import { formatBytes, formatEta, displayName } from '../utils/format'

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
}>()

const emit = defineEmits<{
  (e: 'pause', id: string): void
  (e: 'resume', id: string): void
  (e: 'cancel', id: string): void
  (e: 'retry', id: string): void
  (e: 'resumeAll'): void
}>()

/** 状态 → chip 样式（四色体系） */
const CHIP_CLASS: Record<TaskStateName, string> = {
  queued: 'ft-chip--queued',
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

/** 失败/拒绝原因文案（复用 spec §10 错误 key） */
function reasonText(task: Task): string {
  if (task.state === 'rejected') return t('transfer.error.duplicateName')
  if (task.state === 'failed' && task.reason === 'duplicate-name') {
    return t('transfer.error.duplicateName')
  }
  if (task.state === 'failed' && task.reason === 'remote-changed') {
    return t('transfer.error.remoteChanged')
  }
  return task.state === 'failed' && task.reason ? task.reason : ''
}
</script>

<template>
  <div class="ft-queue">
    <div class="ft-queue-body">
      <!-- 面板头：传输队列 + 任务总数（常驻入口标识，任何状态下可达） -->
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

      <!-- 状态汇总 chips -->
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

      <!-- 空队列 -->
      <div v-if="tasks.length === 0" class="ft-empty">
        {{ t('transfer.task.empty') }}
      </div>

      <!-- 任务卡列表 -->
      <div v-else class="ft-task-list">
        <div v-for="task in tasks" :key="task.id" class="ft-task">
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
        </div>
      </div>
    </div>
  </div>
</template>
