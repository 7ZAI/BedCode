<script setup lang="ts">
/**
 * TaskQueueSheet — 底部抽屉传输队列 (Mobile)
 *
 * 抓把 + 任务卡列表；状态按 spec 9.3 四色体系呈现：
 *   传输中（蓝/绿）｜已暂停（琥珀，▶ 恢复）｜失败（红，重新排队）｜
 *   同名被拒（紫 chip）｜排队（灰）。
 * 任务卡内嵌进度条与暂停/恢复/取消/重新排队操作。
 *
 * 视觉语言统一：复用宿主 group-card / group-row / status-badge / icon-chip，
 * 字号全部 clamp() 流式缩放。
 */
import { computed } from 'vue'
import type { Task } from '../types'
import { TASK_STATE_KEYS, TASK_STATE_COLOR_CLASS, TASK_STATE_PROGRESS_CLASS, isTerminalState } from '../types'
import { formatBytes, formatSpeed, progressPercent } from '../utils/format'

type Translate = (key: string, params?: Record<string, any>) => string

const props = defineProps<{
  open: boolean
  tasks: Task[]
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
  (e: 'resume-all'): void
}>()

const t = props.t

/** 活跃任务数（供标题角标） */
const activeCount = computed(
  () => props.tasks.filter(tk => !isTerminalState(tk.state)).length,
)

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

/** 失败/拒绝原因文案 */
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
  return btns
}

/** 任务方向 → icon-chip 配色 */
function directionChipClass(direction: string): string {
  return direction === 'download' ? 'chip-cyan' : 'chip-amber'
}

/** 任务方向 → SVG path */
function directionIconPath(direction: string): string {
  return direction === 'download'
    ? 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4'
    : 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12'
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
              v-if="resumableCount > 0"
              class="flex-shrink-0 ft-resume-all-btn"
              @click="emit('resume-all')"
            >
              {{ t('transfer.task.resumeAll') }}
            </button>
          </div>

          <!-- 任务卡列表 -->
          <div class="flex-1 overflow-y-auto min-h-0 px-4 pb-[calc(var(--safe-area-bottom,0px)+12px)]">
            <div v-if="tasks.length === 0" class="py-10 text-center">
              <p class="ft-task-empty">{{ t('transfer.task.empty') }}</p>
            </div>

            <div v-for="task in tasks" :key="task.id" class="group-card mb-3">
              <!-- 首行：方向图标 + 名称 + 状态 chip -->
              <div class="group-row" style="gap: 0.625rem">
                <span class="icon-chip flex-shrink-0" :class="directionChipClass(task.direction)">
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
              <div v-if="!isTerminalState(task.state)" class="px-4 pb-3">
                <div class="ft-progress-track">
                  <div
                    class="h-full rounded-full transition-all duration-300"
                    :class="TASK_STATE_PROGRESS_CLASS[task.state]"
                    :style="{ width: taskPercent(task) + '%' }"
                  ></div>
                </div>
              </div>

              <!-- 失败/拒绝原因 -->
              <div v-if="taskReason(task)" class="px-4 pb-2">
                <p class="ft-task-reason">{{ taskReason(task) }}</p>
              </div>

              <!-- 操作按钮 -->
              <div v-if="actionButtons(task).length > 0" class="px-4 pb-3 flex gap-2">
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
  font-size: clamp(0.9375rem, 1rem + (100vw - 360px) / 800 * 0.0625rem, 1.0625rem);
  font-weight: 600;
}

/* 全部恢复按钮 */
.ft-resume-all-btn {
  padding: 0.25rem 0.75rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
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
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800 * 0.0625rem, 0.9375rem);
  color: var(--mobile-text-muted);
}

/* 任务名称 */
.ft-task-name {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800 * 0.0625rem, 0.9375rem);
  font-weight: 500;
}

/* 任务元信息 */
.ft-task-meta {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 进度条轨道 */
.ft-progress-track {
  height: 0.375rem;
  border-radius: 9999px;
  background: var(--mobile-bg-tertiary);
  overflow: hidden;
}

/* 失败原因 */
.ft-task-reason {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  color: var(--mobile-error);
}

/* 操作按钮 */
.ft-task-action-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800 * 0.0625rem, 0.8125rem);
  font-weight: 500;
  transition: opacity 0.15s ease;
}

.ft-task-action-btn:active {
  opacity: 0.8;
}
</style>
