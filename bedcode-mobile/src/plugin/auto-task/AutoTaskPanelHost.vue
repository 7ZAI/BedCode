<template>
  <Teleport to="body">
    <Transition name="bottom-sheet">
      <div v-if="visible" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui">
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="$emit('close')"></div>
        <div class="panel">
          <!-- Header -->
          <div class="panel-header">
            <div class="panel-header-left">
              <h3 class="panel-title">{{ $t('mobile.autoTask.title') }}</h3>
              <span v-if="queue.length > 0" class="queue-badge">{{ queue.length }}</span>
            </div>
            <div class="panel-header-right">
              <button
                v-if="queue.length > 0 && !confirmingClear"
                class="clear-btn"
                @click="handleClear"
              >{{ $t('mobile.autoTask.clear') }}</button>
              <button class="close-btn" @click="$emit('close')">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>

          <!-- 清空确认 -->
          <div v-if="confirmingClear" class="confirm-bar">
            <span class="confirm-text">{{ $t('mobile.autoTask.clearConfirm') }}</span>
            <div class="confirm-actions">
              <button class="confirm-btn confirm-btn-danger" @click="confirmClear">{{ $t('mobile.autoTask.confirm') }}</button>
              <button class="confirm-btn" @click="confirmingClear = false">{{ $t('mobile.autoTask.cancel') }}</button>
            </div>
          </div>

          <!-- 错误提示 -->
          <div v-if="errorMessage" class="error-bar">
            <span class="error-text">{{ errorMessage }}</span>
            <button class="error-close" @click="errorMessage = ''">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <!-- 当前任务状态 -->
          <div v-if="activeSessionId" class="status-section">
            <div v-if="displayTask" class="status-row">
              <span class="status-dot" :style="{ background: statusColor[displayTask.status] || statusColor.idle }"></span>
              <span class="status-label" :style="{ color: statusColor[displayTask.status] || statusColor.idle }">
                {{ statusLabel[displayTask.status] || displayTask.status }}
              </span>
              <p v-if="displayTask.description" class="status-desc">{{ displayTask.description }}</p>
            </div>
            <div v-else class="status-row status-idle">
              <span class="status-dot" :style="{ background: statusColor.idle }"></span>
              <span class="status-label" :style="{ color: statusColor.idle }">{{ $t('mobile.autoTask.idle') }}</span>
            </div>
          </div>

          <!-- 自动执行开关 -->
          <div v-if="activeSessionId" class="toggle-section">
            <div class="toggle-row">
              <div class="toggle-info">
                <p class="toggle-label">{{ $t('mobile.autoTask.autoExecute') }}</p>
                <p class="toggle-hint">{{ $t('mobile.autoTask.autoExecuteHint') }}</p>
              </div>
              <button
                role="switch"
                :aria-checked="autoExecute"
                class="toggle-switch"
                :class="{ on: autoExecute }"
                @click="toggleAutoExecute"
              >
                <span class="toggle-dot"></span>
              </button>
            </div>
            <div class="toggle-row">
              <div class="toggle-info">
                <p class="toggle-label">{{ $t('mobile.autoTask.autoAnswer') }}</p>
                <p class="toggle-hint">{{ $t('mobile.autoTask.autoAnswerHint') }}</p>
              </div>
              <button
                role="switch"
                :aria-checked="autoAnswer"
                class="toggle-switch"
                :class="{ on: autoAnswer }"
                @click="toggleAutoAnswer"
              >
                <span class="toggle-dot"></span>
              </button>
            </div>
          </div>

          <!-- 从预设添加 -->
          <div v-if="presetTasks.length > 0" class="preset-section">
            <h4 class="section-label">{{ $t('mobile.autoTask.addFromPreset') }}</h4>
            <div class="preset-list">
              <button
                v-for="task in presetTasks"
                :key="task.id"
                class="preset-chip"
                :disabled="!activeSessionId"
                @click="handleAddFromPreset(task)"
              >{{ task.content }}</button>
            </div>
          </div>

          <!-- Manual Input -->
          <div class="input-section">
            <div class="input-row">
              <input
                v-model="manualInput"
                type="text"
                :placeholder="$t('mobile.autoTask.inputPlaceholder')"
                class="manual-input"
                :disabled="!activeSessionId"
                @keydown.enter="handleAddManual"
              />
              <button
                class="add-btn"
                :disabled="!activeSessionId || !manualInput.trim()"
                @click="handleAddManual"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                </svg>
              </button>
            </div>
          </div>

          <!-- Queue List -->
          <div class="queue-section">
            <div v-if="loading" class="empty-state">
              <p class="empty-text">{{ $t('common.status.loading') }}</p>
            </div>
            <div v-else-if="queue.length === 0" class="empty-state">
              <svg class="w-8 h-8 mx-auto mb-2 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
              </svg>
              <p class="empty-text">{{ $t('mobile.autoTask.emptyQueue') }}</p>
              <p class="empty-hint">{{ $t('mobile.autoTask.emptyHint') }}</p>
            </div>
            <div v-else class="queue-list">
              <div v-for="(task, index) in queue" :key="task.id" class="queue-item">
                <div class="queue-item-main">
                  <span class="queue-item-position">{{ task.position + 1 }}</span>
                  <!-- 编辑模式 -->
                  <input
                    v-if="editingId === task.id"
                    v-model="editingText"
                    class="edit-input"
                    type="text"
                    @keydown.enter="saveEdit"
                    @keydown.escape="editingId = null"
                    ref="editInputRef"
                  />
                  <p v-else class="queue-item-prompt">{{ task.prompt }}</p>
                </div>
                <div class="queue-item-actions">
                  <!-- 编辑模式按钮 -->
                  <template v-if="editingId === task.id">
                    <button class="action-btn action-btn-primary" @click="saveEdit">
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                      </svg>
                    </button>
                    <button class="action-btn" @click="editingId = null">
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </template>
                  <!-- 正常模式按钮 -->
                  <template v-else>
                    <button
                      class="action-btn"
                      :disabled="index === 0"
                      :title="$t('mobile.autoTask.moveUp')"
                      @click="handleMove(index, -1)"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                      </svg>
                    </button>
                    <button
                      class="action-btn"
                      :disabled="index === queue.length - 1"
                      :title="$t('mobile.autoTask.moveDown')"
                      @click="handleMove(index, 1)"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                      </svg>
                    </button>
                    <button class="action-btn" :title="$t('mobile.autoTask.edit')" @click="startEdit(task)">
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                    </button>
                    <button class="action-btn action-btn-danger" :title="$t('mobile.autoTask.delete')" @click="handleRemove(task.id)">
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </button>
                  </template>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * AutoTaskPanelHost — 自动任务队列面板（宿主侧）
 *
 * 功能对齐桌面端 AutoTaskModal：
 * - 当前任务状态显示
 * - 自动执行 / 自动应答 开关
 * - 队列增删改查 + 排序
 * - 预设任务（移动端独立来源，localStorage 持久化）
 */
import { ref, computed, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  httpTaskQueueList,
  httpTaskQueueAdd,
  httpTaskQueueRemove,
  httpTaskQueueClear,
  httpTaskQueueUpdate,
  httpTaskQueueReorder,
  httpSessionSettings,
  httpSetSessionMode,
  httpCurrentTask,
} from '@/composables/useHttpApi'
import type { AutoTaskQueueItem, QueueListResponse } from '@/composables/useHttpApi'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const toast = useToast()

const props = defineProps<{
  visible: boolean
  activeSessionId: string
}>()

defineEmits<{ close: [] }>()

const { tasks: presetTasks } = usePresetTasks()
const queue = ref<AutoTaskQueueItem[]>([])
const loading = ref(false)
const manualInput = ref('')

// 当前任务状态
interface CurrentTask {
  id: string
  description: string | null
  status: string
  created_at: string
}
const currentTask = ref<CurrentTask | null>(null)

// 开关
const autoExecute = ref(false)
const autoAnswer = ref(false)

// 编辑
const editingId = ref<string | null>(null)
const editingText = ref('')
const editInputRef = ref<HTMLInputElement | null>(null)

// 确认 / 错误
const confirmingClear = ref(false)
const errorMessage = ref('')

// 状态显示
const statusLabel: Record<string, string> = {
  idle: t('mobile.autoTask.idle'),
  in_progress: t('mobile.autoTask.inProgress'),
  asking: t('mobile.autoTask.asking'),
  completed: t('mobile.autoTask.completed'),
  interrupted: t('mobile.autoTask.interrupted'),
  pending: t('mobile.autoTask.pending'),
}

const statusColor: Record<string, string> = {
  idle: 'var(--mobile-text-disabled)',
  in_progress: 'var(--mobile-accent)',
  asking: '#f59e0b',
  completed: '#22c55e',
  interrupted: 'var(--mobile-error)',
  pending: 'var(--mobile-text-disabled)',
}

const displayTask = computed(() => {
  if (currentTask.value && ['in_progress', 'asking'].includes(currentTask.value.status)) {
    return currentTask.value
  }
  return null
})

function showError(message: string) {
  errorMessage.value = message
}

// ==================== Data Loading ====================

async function loadQueue() {
  if (!props.activeSessionId) return
  try {
    const result = await httpTaskQueueList(props.activeSessionId)
    if (result.code === 0 && result.data) {
      queue.value = (result.data as QueueListResponse).tasks || []
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load queue:', e)
    showError(t('mobile.autoTask.loadFailed'))
  }
}

async function loadCurrentTask() {
  if (!props.activeSessionId) return
  try {
    const result = await httpCurrentTask(props.activeSessionId)
    if (result.code === 0 && result.data) {
      currentTask.value = result.data.task as CurrentTask | null
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load current task:', e)
  }
}

async function loadSessionSettings() {
  if (!props.activeSessionId) return
  try {
    const result = await httpSessionSettings(props.activeSessionId)
    if (result.code === 0 && result.data) {
      autoExecute.value = result.data.auto_execute === true
      autoAnswer.value = result.data.auto_answer === true
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load session settings:', e)
  }
}

async function refresh() {
  if (!props.activeSessionId) return
  loading.value = true
  try {
    await Promise.all([loadQueue(), loadCurrentTask(), loadSessionSettings()])
  } finally {
    loading.value = false
  }
}

// 面板打开时加载
watch(() => props.visible, (val) => {
  if (val && props.activeSessionId) {
    refresh()
  }
})

// ==================== Actions ====================

async function handleAddFromPreset(task: any) {
  if (!props.activeSessionId) return
  const result = await httpTaskQueueAdd(props.activeSessionId, task.content)
  if (result.code === 0) {
    await loadQueue()
  } else {
    showError(t('mobile.autoTask.addFailed'))
  }
}

async function handleAddManual() {
  if (!props.activeSessionId || !manualInput.value.trim()) return
  const result = await httpTaskQueueAdd(props.activeSessionId, manualInput.value.trim())
  if (result.code === 0) {
    manualInput.value = ''
    await loadQueue()
  } else {
    showError(t('mobile.autoTask.addFailed'))
  }
}

async function handleRemove(taskId: string) {
  if (!props.activeSessionId) return
  const result = await httpTaskQueueRemove(props.activeSessionId, taskId)
  if (result.code === 0) {
    await loadQueue()
  } else {
    showError(t('mobile.autoTask.removeFailed'))
  }
}

function handleClear() {
  if (!props.activeSessionId || queue.value.length === 0) return
  confirmingClear.value = true
}

async function confirmClear() {
  confirmingClear.value = false
  if (!props.activeSessionId) return
  const result = await httpTaskQueueClear(props.activeSessionId)
  if (result.code === 0) {
    queue.value = []
  } else {
    showError(t('mobile.autoTask.clearFailed'))
  }
}

// ==================== Edit ====================

function startEdit(task: AutoTaskQueueItem) {
  editingId.value = task.id
  editingText.value = task.prompt
  nextTick(() => {
    editInputRef.value?.focus()
  })
}

async function saveEdit() {
  const prompt = editingText.value.trim()
  if (!props.activeSessionId || !editingId.value || !prompt) {
    editingId.value = null
    return
  }
  const result = await httpTaskQueueUpdate(props.activeSessionId, editingId.value, prompt)
  if (result.code === 0) {
    editingId.value = null
    await loadQueue()
  } else {
    showError(t('mobile.autoTask.updateFailed'))
  }
}

// ==================== Reorder ====================

async function handleMove(index: number, direction: -1 | 1) {
  const target = index + direction
  if (target < 0 || target >= queue.value.length) return
  const items = [...queue.value]
  const [item] = items.splice(index, 1)
  items.splice(target, 0, item)
  await commitReorder(items)
}

async function commitReorder(items: AutoTaskQueueItem[]) {
  if (!props.activeSessionId) return
  const taskIds = items.map(i => i.id)
  const result = await httpTaskQueueReorder(props.activeSessionId, taskIds)
  if (result.code === 0) {
    queue.value = items.map((item, idx) => ({ ...item, position: idx }))
  } else {
    showError(t('mobile.autoTask.reorderFailed'))
    await loadQueue()
  }
}

// ==================== Toggles ====================

async function toggleAutoExecute() {
  if (!props.activeSessionId) return
  const target = !autoExecute.value
  const result = await httpSetSessionMode(props.activeSessionId, target, undefined)
  if (result.code === 0) {
    autoExecute.value = target
  } else {
    showError(t('mobile.autoTask.modeFailed'))
  }
}

async function toggleAutoAnswer() {
  if (!props.activeSessionId) return
  const target = !autoAnswer.value
  const result = await httpSetSessionMode(props.activeSessionId, undefined, target)
  if (result.code === 0) {
    autoAnswer.value = target
  } else {
    showError(t('mobile.autoTask.modeFailed'))
  }
}
</script>

<style scoped>
.panel {
  position: relative;
  width: 100%;
  max-height: 80vh;
  background: var(--mobile-bg-secondary);
  border-top-left-radius: 1.25rem;
  border-top-right-radius: 1.25rem;
  border-top: 1px solid var(--mobile-border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 1.25rem 0.75rem;
  flex-shrink: 0;
}

.panel-header-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.panel-header-right {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.panel-title {
  font-size: 1.0625rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
}

.queue-badge {
  padding: 0.0625rem 0.375rem;
  border-radius: 0.5rem;
  background: var(--mobile-accent);
  color: #fff;
  font-size: 0.6875rem;
  font-weight: 600;
  min-width: 1.125rem;
  text-align: center;
}

.clear-btn {
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  background: color-mix(in srgb, var(--mobile-error) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--mobile-error) 20%, transparent);
  color: var(--mobile-error);
  font-size: 0.75rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.clear-btn:hover {
  background: color-mix(in srgb, var(--mobile-error) 20%, transparent);
}

.close-btn {
  padding: 0.375rem;
  color: var(--mobile-text-muted);
  background: none;
  border: none;
  cursor: pointer;
  border-radius: 0.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.15s ease;
}

.close-btn:hover {
  color: var(--mobile-text-primary);
}

/* 确认条 */
.confirm-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 1.25rem;
  background: color-mix(in srgb, var(--mobile-error) 8%, transparent);
  border-top: 1px solid color-mix(in srgb, var(--mobile-error) 15%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--mobile-error) 15%, transparent);
}

.confirm-text {
  font-size: 0.8125rem;
  color: var(--mobile-error);
}

.confirm-actions {
  display: flex;
  gap: 0.375rem;
}

.confirm-btn {
  padding: 0.25rem 0.625rem;
  border-radius: 0.375rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  font-size: 0.75rem;
  cursor: pointer;
  transition: all 0.15s ease;
}

.confirm-btn-danger {
  background: var(--mobile-error);
  border-color: var(--mobile-error);
  color: #fff;
}

/* 错误条 */
.error-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 1.25rem;
  background: color-mix(in srgb, var(--mobile-error) 8%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--mobile-error) 15%, transparent);
}

.error-text {
  font-size: 0.8125rem;
  color: var(--mobile-error);
}

.error-close {
  padding: 0.25rem;
  color: var(--mobile-error);
  background: none;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
}

/* 当前任务状态 */
.status-section {
  padding: 0.75rem 1.25rem;
  flex-shrink: 0;
  border-bottom: 1px solid var(--mobile-border);
}

.status-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.status-dot {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-label {
  font-size: 0.8125rem;
  font-weight: 500;
}

.status-desc {
  width: 100%;
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
  margin: 0.25rem 0 0 1rem;
  line-height: 1.4;
}

/* 开关 */
.toggle-section {
  padding: 0.75rem 1.25rem;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  border-bottom: 1px solid var(--mobile-border);
}

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.toggle-info {
  flex: 1;
  min-width: 0;
}

.toggle-label {
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--mobile-text-primary);
  margin: 0;
}

.toggle-hint {
  font-size: 0.6875rem;
  color: var(--mobile-text-muted);
  margin: 0.125rem 0 0;
}

.toggle-switch {
  position: relative;
  width: 2.75rem;
  height: 1.5rem;
  border-radius: 0.75rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border-hover);
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
  padding: 0;
}

.toggle-switch.on {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
}

.toggle-dot {
  position: absolute;
  top: 0.125rem;
  left: 0.125rem;
  width: 1.125rem;
  height: 1.125rem;
  border-radius: 50%;
  background: #fff;
  transition: transform 0.2s ease;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.toggle-switch.on .toggle-dot {
  transform: translateX(1.25rem);
}

/* 预设任务 */
.preset-section {
  padding: 0.75rem 1.25rem;
  flex-shrink: 0;
}

.section-label {
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--mobile-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0 0 0.5rem;
}

.preset-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
}

.preset-chip {
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  font-size: 0.75rem;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.preset-chip:hover {
  border-color: rgba(0, 212, 255, 0.3);
  color: var(--mobile-accent);
}

.preset-chip:active {
  transform: scale(0.95);
}

.preset-chip:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* 输入 */
.input-section {
  padding: 0 1.25rem 0.75rem;
  flex-shrink: 0;
}

.input-row {
  display: flex;
  gap: 0.5rem;
}

.manual-input {
  flex: 1;
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border-hover);
  border-radius: 0.625rem;
  padding: 0.5rem 0.75rem;
  color: var(--mobile-text-primary);
  font-size: 0.8125rem;
  outline: none;
  transition: border-color 0.15s ease;
}

.manual-input:focus {
  border-color: var(--mobile-accent);
}

.manual-input::placeholder {
  color: var(--mobile-text-disabled);
}

.manual-input:disabled {
  opacity: 0.4;
}

.add-btn {
  flex-shrink: 0;
  padding: 0.5rem;
  border-radius: 0.625rem;
  background: var(--mobile-accent-secondary);
  border: 1px solid var(--mobile-border-active);
  color: var(--mobile-accent);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
}

.add-btn:hover {
  background: color-mix(in srgb, var(--mobile-accent) 30%, transparent);
}

.add-btn:active {
  transform: scale(0.95);
}

.add-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

/* 队列 */
.queue-section {
  flex: 1;
  overflow-y: auto;
  padding: 0 1.25rem 1.25rem;
  min-height: 0;
}

.queue-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.queue-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.625rem;
  transition: all 0.15s ease;
}

.queue-item-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
}

.queue-item-position {
  flex-shrink: 0;
  width: 1.25rem;
  height: 1.25rem;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.625rem;
  font-weight: 600;
  color: var(--mobile-accent);
  background: color-mix(in srgb, var(--mobile-accent) 15%, transparent);
  border-radius: 0.25rem;
  margin-top: 0.0625rem;
}

.queue-item-prompt {
  flex: 1;
  min-width: 0;
  font-size: 0.8125rem;
  color: var(--mobile-text-primary);
  line-height: 1.4;
  word-break: break-word;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.edit-input {
  flex: 1;
  min-width: 0;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-accent);
  border-radius: 0.375rem;
  padding: 0.25rem 0.5rem;
  color: var(--mobile-text-primary);
  font-size: 0.8125rem;
  outline: none;
}

.queue-item-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-shrink: 0;
}

.action-btn {
  padding: 0.25rem;
  color: var(--mobile-text-muted);
  background: none;
  border: none;
  cursor: pointer;
  border-radius: 0.375rem;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.action-btn:hover {
  color: var(--mobile-text-primary);
  background: var(--mobile-bg-hover);
}

.action-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.action-btn-primary:hover {
  color: var(--mobile-accent);
}

.action-btn-danger:hover {
  color: var(--mobile-error);
  background: color-mix(in srgb, var(--mobile-error) 10%, transparent);
}

.empty-state {
  text-align: center;
  padding: 2rem 0;
}

.empty-text {
  color: var(--mobile-text-disabled);
  font-size: 0.875rem;
  margin: 0;
}

.empty-hint {
  color: var(--mobile-text-disabled);
  font-size: 0.75rem;
  margin: 0.25rem 0 0 0;
  opacity: 0.7;
}

/* Bottom sheet transition */
.bottom-sheet-enter-active,
.bottom-sheet-leave-active {
  transition: transform 0.3s cubic-bezier(0.32, 0.72, 0, 1), opacity 0.2s ease;
}

.bottom-sheet-enter-from,
.bottom-sheet-leave-to {
  transform: translateY(100%);
  opacity: 0;
}
</style>
