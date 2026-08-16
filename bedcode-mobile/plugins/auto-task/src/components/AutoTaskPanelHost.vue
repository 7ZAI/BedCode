<template>
  <Transition name="atp-sheet">
    <div v-if="autoTaskPanelVisible" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="close"></div>
      <div class="atp-panel" :style="panelStyle">
        <!-- Header -->
        <div class="atp-panel-header">
          <div class="atp-panel-header-left">
            <h3 class="atp-panel-title">{{ t('title') }}</h3>
            <span v-if="queue.length > 0" class="atp-queue-badge">{{ queue.length }}</span>
          </div>
          <div class="atp-panel-header-right">
            <button
              v-if="queue.length > 0 && !confirmingClear"
              class="atp-clear-btn"
              @click="handleClear"
            >{{ t('clear') }}</button>
            <button class="atp-close-btn" @click="close">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- 清空确认 -->
        <div v-if="confirmingClear" class="atp-confirm-bar">
          <span class="atp-confirm-text">{{ t('clearConfirm') }}</span>
          <div class="atp-confirm-actions">
            <button class="atp-confirm-btn atp-confirm-btn-danger" @click="confirmClear">{{ t('confirm') }}</button>
            <button class="atp-confirm-btn" @click="confirmingClear = false">{{ t('cancel') }}</button>
          </div>
        </div>

        <!-- 错误提示 -->
        <div v-if="errorMessage" class="atp-error-bar">
          <span class="atp-error-text">{{ errorMessage }}</span>
          <button class="atp-error-close" @click="errorMessage = ''">
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- 当前任务状态 -->
        <div v-if="activeSessionId" class="atp-status-section">
          <div v-if="displayTask" class="atp-status-row">
            <span class="atp-status-dot" :style="{ background: statusColor[displayTask.status] || statusColor.idle }"></span>
            <span class="atp-status-label" :style="{ color: statusColor[displayTask.status] || statusColor.idle }">
              {{ statusLabel[displayTask.status] || displayTask.status }}
            </span>
            <p v-if="displayTask.description" class="atp-status-desc">{{ displayTask.description }}</p>
            <!-- 活动队列项（waiting/executing）可取消：用户中途放弃长任务的唯一入口 -->
            <button
              v-if="activeTask && ['waiting', 'executing'].includes(activeTask.status)"
              class="atp-action-btn atp-action-btn-danger atp-cancel-task-btn"
              :title="t('cancelTask')"
              @click="handleCancelTask(activeTask.id)"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
          <div v-else class="atp-status-row atp-status-idle">
            <span class="atp-status-dot" :style="{ background: statusColor.idle }"></span>
            <span class="atp-status-label" :style="{ color: statusColor.idle }">{{ t('idle') }}</span>
          </div>
        </div>

        <!-- 自动执行开关 -->
        <div v-if="activeSessionId" class="atp-toggle-section">
          <div class="atp-toggle-row">
            <div class="atp-toggle-info">
              <p class="atp-toggle-label">{{ t('autoExecute') }}</p>
              <p class="atp-toggle-hint">{{ t('autoExecuteHint') }}</p>
            </div>
            <button
              role="switch"
              :aria-checked="autoExecute"
              class="atp-toggle-switch"
              :class="{ on: autoExecute }"
              @click="toggleAutoExecute"
            >
              <span class="atp-toggle-dot"></span>
            </button>
          </div>
          <div class="atp-toggle-row">
            <div class="atp-toggle-info">
              <p class="atp-toggle-label">{{ t('autoAnswer') }}</p>
              <p class="atp-toggle-hint">{{ t('autoAnswerHint') }}</p>
            </div>
            <button
              role="switch"
              :aria-checked="autoAnswer"
              class="atp-toggle-switch"
              :class="{ on: autoAnswer }"
              @click="toggleAutoAnswer"
            >
              <span class="atp-toggle-dot"></span>
            </button>
          </div>
        </div>

        <!-- 从预设添加 -->
        <div v-if="presetTasks.length > 0" class="atp-preset-section">
          <h4 class="atp-section-label">{{ t('addFromPreset') }}</h4>
          <div class="atp-preset-list">
            <button
              v-for="task in presetTasks"
              :key="task.id"
              class="atp-preset-chip"
              :disabled="!activeSessionId || !canEnqueue(task)"
              @click="handleAddFromPreset(task)"
            >
              <span class="atp-preset-chip-text">{{ task.content }}</span>
              <span
                class="atp-preset-chip-status"
                :style="{ color: statusColor[task.status] || statusColor.idle }"
              >{{ presetStatusLabel(task.status) }}</span>
            </button>
          </div>
        </div>

        <!-- Manual Input -->
        <div class="atp-input-section">
          <div class="atp-input-row">
            <input
              v-model="manualInput"
              type="text"
              :placeholder="t('inputPlaceholder')"
              class="atp-manual-input"
              :disabled="!activeSessionId"
              @keydown.enter="handleAddManual"
            />
            <button
              class="atp-add-btn"
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
        <div class="atp-queue-section">
          <div v-if="loading" class="atp-empty-state">
            <p class="atp-empty-text">{{ t('loading') }}</p>
          </div>
          <div v-else-if="queue.length === 0" class="atp-empty-state">
            <svg class="w-8 h-8 mx-auto mb-2 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
            </svg>
            <p class="atp-empty-text">{{ t('emptyQueue') }}</p>
            <p class="atp-empty-hint">{{ t('emptyHint') }}</p>
          </div>
          <div v-else class="atp-queue-list">
            <div v-for="(task, index) in queue" :key="task.id" class="atp-queue-item">
              <div class="atp-queue-item-main">
                <span class="atp-queue-item-position">{{ task.position + 1 }}</span>
                <!-- 编辑模式 -->
                <input
                  v-if="editingId === task.id"
                  v-model="editingText"
                  class="atp-edit-input"
                  type="text"
                  @keydown.enter="saveEdit"
                  @keydown.escape="editingId = null"
                  ref="editInputRef"
                />
                <p v-else class="atp-queue-item-prompt">{{ task.prompt }}</p>
              </div>
              <div class="atp-queue-item-actions">
                <!-- 编辑模式按钮 -->
                <template v-if="editingId === task.id">
                  <button class="atp-action-btn atp-action-btn-primary" @click="saveEdit">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                    </svg>
                  </button>
                  <button class="atp-action-btn" @click="editingId = null">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </template>
                <!-- 正常模式按钮 -->
                <template v-else>
                  <button
                    class="atp-action-btn"
                    :disabled="index === 0"
                    :title="t('moveUp')"
                    @click="handleMove(index, -1)"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                    </svg>
                  </button>
                  <button
                    class="atp-action-btn"
                    :disabled="index === queue.length - 1"
                    :title="t('moveDown')"
                    @click="handleMove(index, 1)"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                    </svg>
                  </button>
                  <button class="atp-action-btn" :title="t('edit')" @click="startEdit(task)">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    </svg>
                  </button>
                  <button class="atp-action-btn atp-action-btn-danger" :title="t('delete')" @click="handleRemove(task.id)">
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
</template>

<script setup lang="ts">
/**
 * AutoTaskPanelHost — 自动任务队列面板（插件侧）
 *
 * 原宿主侧实现迁入插件（对齐桌面端 AutoTaskModal 的挂载方式）：
 * - 面板组件随插件 bundle 分发，经 createApp + document.body.appendChild 挂载
 * - 通过 inject('pluginContext') 使用 SDK 能力（i18n / dialogs）
 * - 当前活动会话与队列 HTTP 接口经 SDK getMobileApi() 访问宿主共享运行时
 * - 键盘避让：与 TerminalView 一致的双通道检测（visualViewport + 插件 safeAreaChanged
 *   DOM 事件），面板整体上移避开系统输入法
 */
import { ref, computed, watch, nextTick, inject, onMounted, onUnmounted } from 'vue'
import { getMobileApi, getPresetTasks } from '@bedcode/plugin-sdk-mobile'
import type { PluginContext, MobileHostApi } from '@bedcode/plugin-sdk-mobile'
import { autoTaskPanelVisible } from '../state'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string): string => context.i18n.t(key)
const mobileApi = getMobileApi() as MobileHostApi

// ==================== Types ====================

interface QueueTaskItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

interface CurrentTask {
  id: string
  description: string | null
  status: string
  created_at: string
}

// ==================== State ====================

const { tasks: presetTasks, markEnqueued, markCompletedByTaskId, markInterruptedByTaskId, revertToUnusedByTaskId, reconcileWithQueue, canEnqueue } = getPresetTasks().usePresetTasks()
const queue = ref<QueueTaskItem[]>([])
// 当前处理中的队列项（waiting/executing）：displayTask 的补充来源，
// 队列项进入 waiting/executing 后不在 pending 列表，但仍在处理中需要展示
const activeTask = ref<QueueTaskItem | null>(null)
const loading = ref(false)
const manualInput = ref('')

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

// 当前活动会话（宿主共享 ref，进入终端会话时由宿主更新）
const activeSessionId = computed(() => mobileApi.activeSessionId.value ?? '')

// 状态显示
const statusLabel: Record<string, string> = {
  idle: t('idle'),
  in_progress: t('inProgress'),
  asking: t('asking'),
  completed: t('completed'),
  interrupted: t('interrupted'),
  pending: t('pending'),
  // 队列项处理中状态（active_task）
  waiting: t('waiting'),
  // 预设任务执行状态（本地，入队即视为已执行）
  unused: t('unused'),
  executing: t('inProgress'),
}

/** 预设任务状态标签（未使用/执行中/已完成/已中断） */
function presetStatusLabel(status: string): string {
  return statusLabel[status] || status
}

const statusColor: Record<string, string> = {
  idle: 'var(--mobile-text-disabled)',
  in_progress: 'var(--mobile-accent)',
  asking: '#f59e0b',
  completed: '#22c55e',
  interrupted: 'var(--mobile-error)',
  pending: 'var(--mobile-text-disabled)',
  // 队列项处理中状态（active_task）
  waiting: '#f59e0b',
  // 预设任务执行状态（本地，入队即视为已执行）
  unused: 'var(--mobile-text-disabled)',
  executing: 'var(--mobile-accent)',
}

/** 状态区显示：优先任务历史（执行中/等待输入），否则队列处理中项（等待下发/已下发） */
const displayTask = computed(() => {
  const t =
    currentTask.value && ['in_progress', 'asking'].includes(currentTask.value.status)
      ? currentTask.value
      : activeTask.value
  if (!t) return null
  return {
    status: t.status,
    // QueueTaskItem 用 prompt 承载描述，CurrentTask 用 description
    description: (t as any).description ?? (t as any).prompt ?? null,
  }
})

function showError(message: string) {
  errorMessage.value = message
}

function close() {
  autoTaskPanelVisible.value = false
}

// ==================== Data Loading ====================

async function loadQueue() {
  if (!activeSessionId.value) return
  try {
    const result = await mobileApi.httpTaskQueueList(activeSessionId.value)
    if (result.code === 0 && result.data) {
      queue.value = result.data.tasks || []
      activeTask.value = result.data.active_task || null
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load queue:', e)
    showError(t('loadFailed'))
  }
}

async function loadCurrentTask() {
  if (!activeSessionId.value) return
  try {
    const result = await mobileApi.httpCurrentTask(activeSessionId.value)
    if (result.code === 0 && result.data) {
      currentTask.value = result.data.task as CurrentTask | null
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load current task:', e)
  }
}

async function loadSessionSettings() {
  if (!activeSessionId.value) return
  try {
    const result = await mobileApi.httpSessionSettings(activeSessionId.value)
    if (result.code === 0 && result.data) {
      autoExecute.value = result.data.auto_execute === true
      autoAnswer.value = result.data.auto_answer === true
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load session settings:', e)
  }
}

async function refresh() {
  if (!activeSessionId.value) return
  loading.value = true
  try {
    await Promise.all([loadQueue(), loadCurrentTask(), loadSessionSettings()])
    // 对账：本地执行中的预设，其队列项已不在桌面 pending 队列（完成/被删/丢失）
    // 且未收到完成广播 → 落 interrupted（幂等；pending 列表是权威的排队信号，
    // 仅判定本会话入队的预设，防多会话误中断）
    await reconcileWithQueue(activeSessionId.value)
  } finally {
    loading.value = false
  }
}

// 面板打开时加载
watch(autoTaskPanelVisible, (val) => {
  if (val) {
    refresh()
  }
})

// ==================== Actions ====================

async function handleAddFromPreset(task: any) {
  if (!activeSessionId.value || !canEnqueue(task)) return
  const result = await mobileApi.httpTaskQueueAdd(activeSessionId.value, task.content)
  if (result.code === 0 && result.data?.task_id) {
    // 入队即视为已执行（可靠信号，不等完成广播）：记录队列项 id 与所在会话
    await markEnqueued(task.id, result.data.task_id, activeSessionId.value)
    await loadQueue()
  } else {
    showError(t('addFailed'))
  }
}

async function handleAddManual() {
  if (!activeSessionId.value || !manualInput.value.trim()) return
  const result = await mobileApi.httpTaskQueueAdd(activeSessionId.value, manualInput.value.trim())
  if (result.code === 0) {
    manualInput.value = ''
    await loadQueue()
  } else {
    showError(t('addFailed'))
  }
}

async function handleRemove(taskId: string) {
  if (!activeSessionId.value) return
  const result = await mobileApi.httpTaskQueueRemove(activeSessionId.value, taskId)
  if (result.code === 0) {
    // 队列项移除 → 关联预设回退未使用（可再次添加）
    await revertToUnusedByTaskId(taskId)
    await loadQueue()
  } else {
    showError(t('removeFailed'))
  }
}

/** 取消活动队列项（waiting/executing）：预设状态由 cancel 广播落 interrupted（见 handleTaskQueueChanged） */
async function handleCancelTask(taskId: string) {
  if (!activeSessionId.value) return
  const result = await mobileApi.httpTaskQueueCancel(activeSessionId.value, taskId)
  if (result.code === 0) {
    activeTask.value = null
    await loadQueue()
  } else {
    showError(t('cancelTaskFailed'))
  }
}

function handleClear() {
  if (!activeSessionId.value || queue.value.length === 0) return
  confirmingClear.value = true
}

async function confirmClear() {
  confirmingClear.value = false
  if (!activeSessionId.value) return
  // 清空前快照待清队列项 id：清空成功后逐个回退关联预设
  const clearedIds = queue.value.map(q => q.id)
  const result = await mobileApi.httpTaskQueueClear(activeSessionId.value)
  if (result.code === 0) {
    queue.value = []
    for (const id of clearedIds) {
      await revertToUnusedByTaskId(id)
    }
  } else {
    showError(t('clearFailed'))
  }
}

// ==================== Edit ====================

function startEdit(task: QueueTaskItem) {
  editingId.value = task.id
  editingText.value = task.prompt
  nextTick(() => {
    editInputRef.value?.focus()
  })
}

async function saveEdit() {
  const prompt = editingText.value.trim()
  if (!activeSessionId.value || !editingId.value || !prompt) {
    editingId.value = null
    return
  }
  const result = await mobileApi.httpTaskQueueUpdate(activeSessionId.value, editingId.value, prompt)
  if (result.code === 0) {
    editingId.value = null
    await loadQueue()
  } else {
    showError(t('updateFailed'))
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

async function commitReorder(items: QueueTaskItem[]) {
  if (!activeSessionId.value) return
  const taskIds = items.map(i => i.id)
  const result = await mobileApi.httpTaskQueueReorder(activeSessionId.value, taskIds)
  if (result.code === 0) {
    queue.value = items.map((item, idx) => ({ ...item, position: idx }))
  } else {
    showError(t('reorderFailed'))
    await loadQueue()
  }
}

// ==================== Toggles ====================

async function toggleAutoExecute() {
  if (!activeSessionId.value) return
  const target = !autoExecute.value
  const result = await mobileApi.httpSetSessionMode(activeSessionId.value, target, undefined)
  if (result.code === 0) {
    autoExecute.value = target
  } else {
    showError(t('modeFailed'))
  }
}

async function toggleAutoAnswer() {
  if (!activeSessionId.value) return
  const target = !autoAnswer.value
  const result = await mobileApi.httpSetSessionMode(activeSessionId.value, undefined, target)
  if (result.code === 0) {
    autoAnswer.value = target
  } else {
    showError(t('modeFailed'))
  }
}

// ==================== Keyboard Avoidance ====================
//
// 双通道键盘检测，与 TerminalView 完全一致（Android adjustNothing 下 WebView 不缩放）：
// - 通道 1 (visualViewport): 部分 WebView 键盘弹出时 visualViewport.height 缩小，
//   通过 resize/scroll 事件计算偏移
// - 通道 2 (插件 keyboardHeight): tauri-plugin-edge-to-edge 的 safeAreaChanged DOM 事件
//
// 最终偏移取两个通道较大值，面板整体 translateY 上移，底部恰好对齐键盘顶部。

// 通道 1: visualViewport
const fullLayoutHeight = ref(window.innerHeight)
const viewportHeight = ref(window.visualViewport?.height ?? window.innerHeight)

// 通道 2: 插件报告的键盘高度
const pluginKeyboardHeight = ref(0)

// 最终键盘偏移量：取两个通道中的较大值
const keyboardOffset = computed(() => {
  const vvOffset = fullLayoutHeight.value - viewportHeight.value
  const offset = Math.max(vvOffset, pluginKeyboardHeight.value)
  return offset > 10 ? offset : 0
})

function handleVisualViewportChange() {
  const vv = window.visualViewport
  if (!vv) return
  // 无键盘时更新基准高度
  if (!keyboardOffset.value) {
    fullLayoutHeight.value = window.innerHeight
  }
  viewportHeight.value = vv.height
}

function handlePluginSafeAreaChange(e: Event) {
  const detail = (e as CustomEvent).detail as {
    keyboardHeight: number
    keyboardVisible: boolean
  }
  pluginKeyboardHeight.value = detail.keyboardVisible ? detail.keyboardHeight : 0
}

// 面板样式：键盘弹出时整体上移，并压缩最大高度防止顶部越界
const panelStyle = computed(() => {
  const kb = keyboardOffset.value
  if (kb <= 0) {
    return { maxHeight: '80vh' }
  }
  return {
    transform: `translateY(-${kb}px)`,
    maxHeight: `calc(100dvh - ${kb}px - 0.75rem)`,
  }
})

onMounted(() => {
  // 通道 1: 监听 visualViewport 变化
  if (window.visualViewport) {
    window.visualViewport.addEventListener('resize', handleVisualViewportChange)
    window.visualViewport.addEventListener('scroll', handleVisualViewportChange)
  }
  // 通道 2: 监听插件 safeAreaChanged 事件
  window.addEventListener('safeAreaChanged', handlePluginSafeAreaChange as EventListener)
  // 完成广播：桌面端任务 done 时经宿主转发（bedcode:task_queue_changed），
  // 按队列项 id 匹配本地预设 → completed（不匹配忽略；dev-shell 可手动 dispatch 模拟）
  window.addEventListener('bedcode:task_queue_changed', handleTaskQueueChanged)
})

onUnmounted(() => {
  if (window.visualViewport) {
    window.visualViewport.removeEventListener('resize', handleVisualViewportChange)
    window.visualViewport.removeEventListener('scroll', handleVisualViewportChange)
  }
  window.removeEventListener('safeAreaChanged', handlePluginSafeAreaChange as EventListener)
  window.removeEventListener('bedcode:task_queue_changed', handleTaskQueueChanged)
})

/** 桌面端任务队列变更（宿主转发的 CustomEvent）——完成/取消/回退按 task_id 匹配预设。
 * 不校验 session_id：预设可入队到任意会话（活动会话切换后广播仍应生效），
 * 匹配唯一性由队列项 UUID 保证（手动输入项无本地记录，自然忽略） */
function handleTaskQueueChanged(e: Event) {
  const detail = (e as CustomEvent).detail as {
    session_id: string
    queue_count: number
    action: string
    task_id?: string | null
    status?: string | null
  }
  if (!detail?.task_id) return
  if (detail.action === 'done') {
    markCompletedByTaskId(detail.task_id)
  } else if (detail.action === 'cancel' || detail.action === 'interrupted') {
    // 队列项被放弃（等待超时取消 / 会话终止兜底）：对应预设落中断
    markInterruptedByTaskId(detail.task_id)
  } else if (detail.action === 'revert') {
    // clear 发送失败回退 pending：任务重新排队，刷新队列重新显示
    loadQueue()
  }
}
</script>

