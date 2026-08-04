<script setup lang="ts">
/**
 * TaskQueueSheet — 底部抽屉传输队列 (Mobile)
 *
 * 抓把 + 任务卡列表；状态按 spec 9.3 四色体系呈现：
 *   传输中（蓝/绿）｜已暂停（琥珀，▶ 恢复）｜失败（红，重新排队）｜
 *   同名被拒（紫 chip）｜排队（灰）。
 * 任务卡内嵌进度条与暂停/恢复/取消/重新排队操作。
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
            <h3 class="flex-1 text-base font-semibold text-[var(--mobile-text-primary)]">
              {{ t('transfer.queue.title') }}
            </h3>
            <span v-if="activeCount > 0" class="text-xs text-[var(--mobile-accent)] font-medium">
              {{ t('transfer.queue.active', { count: activeCount }) }}
            </span>
            <button
              v-if="resumableCount > 0"
              class="flex-shrink-0 px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)] active:opacity-80"
              @click="emit('resume-all')"
            >
              {{ t('transfer.task.resumeAll') }}
            </button>
          </div>

          <!-- 任务卡列表 -->
          <div class="flex-1 overflow-y-auto min-h-0 px-4 pb-[calc(var(--safe-area-bottom,0px)+12px)]">
            <div v-if="tasks.length === 0" class="py-10 text-center">
              <p class="text-sm text-[var(--mobile-text-muted)]">{{ t('transfer.task.empty') }}</p>
            </div>

            <div v-for="task in tasks" :key="task.id" class="mb-3 rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)] p-3">
              <!-- 首行：方向图标 + 名称 + 状态 chip -->
              <div class="flex items-center gap-2.5">
                <div
                  class="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
                  :class="task.direction === 'download' ? 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]' : 'bg-[var(--mobile-warning-muted)] text-[var(--mobile-warning)]'"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      v-if="task.direction === 'download'"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                    />
                    <path
                      v-else
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
                    />
                  </svg>
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-sm text-[var(--mobile-text-primary)] truncate">{{ taskName(task) }}</p>
                  <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 truncate">{{ taskMeta(task) }}</p>
                </div>
                <span
                  class="flex-shrink-0 ft-chip"
                  :class="TASK_STATE_COLOR_CLASS[task.state]"
                >
                  {{ t(TASK_STATE_KEYS[task.state]) }}
                </span>
              </div>

              <!-- 进度条（仅非终态） -->
              <div v-if="!isTerminalState(task.state)" class="mt-2.5 h-1.5 rounded-full bg-[var(--mobile-bg-tertiary)] overflow-hidden">
                <div
                  class="h-full rounded-full transition-all duration-300"
                  :class="TASK_STATE_PROGRESS_CLASS[task.state]"
                  :style="{ width: taskPercent(task) + '%' }"
                ></div>
              </div>

              <!-- 失败/拒绝原因 -->
              <p v-if="taskReason(task)" class="mt-2 text-xs text-[var(--mobile-error)]">
                {{ taskReason(task) }}
              </p>

              <!-- 操作按钮 -->
              <div v-if="actionButtons(task).length > 0" class="mt-2.5 flex gap-2">
                <button
                  v-for="btn in actionButtons(task)"
                  :key="btn.key"
                  class="flex-1 py-2 rounded-lg text-xs font-medium active:opacity-80 transition-opacity"
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
