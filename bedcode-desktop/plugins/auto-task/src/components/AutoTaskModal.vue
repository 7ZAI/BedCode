<script setup lang="ts">
/**
 * AutoTaskModal — 自动任务队列弹窗
 *
 * 提供当前任务状态、自动模式开关、队列增删改查与排序。
 * 全部通过插件 context.commands 调用 Rust WASM 后端，监听事件实时刷新。
 * 当前会话从宿主共享运行时 router 的 /terminal-window/:id 路由参数获取。
 */
import { ref, computed, watch, onMounted, onUnmounted, inject, nextTick } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import { autoTaskModalVisible } from '../state'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string) => context.i18n.t(key)

// ==================== Types ====================

interface QueueItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

interface HistoryRecord {
  id: string
  description: string | null
  status: string
  auto_approve: number
  created_at: string
  started_at: string | null
  completed_at: string | null
}

interface PresetItem {
  id: string
  prompt: string
  created_at: string
}

// ==================== State ====================

const visible = autoTaskModalVisible
const queue = ref<QueueItem[]>([])
// 当前处理中的队列项（waiting/executing）：活动任务展示 + 取消入口（与移动端面板对齐）
const activeTask = ref<QueueItem | null>(null)
const currentTask = ref<HistoryRecord | null>(null)
// 预设任务（无会话时在侧边栏创建，弹窗内可选入当前会话队列，加入后自动移除）
const presets = ref<PresetItem[]>([])
const addingPresetId = ref<string | null>(null)
// 两个独立开关：自动执行（入队任务自动调度） / 自动应答（Agent 提问自动回答）
const autoExecute = ref(false)
const autoAnswer = ref(false)
const loading = ref(false)
const manualInput = ref('')
const editingId = ref<string | null>(null)
const editingText = ref('')

// 前端错误提示（替代系统 alert）：banner 展示，可手动关闭
const errorMessage = ref('')
// 清空确认（替代系统 confirm）：改为弹窗内联确认条
const confirmingClear = ref(false)

function showError(message: string, e?: unknown) {
  errorMessage.value = message
  if (e) console.error('[AutoTaskModal]', message, e)
}

function clearError() {
  errorMessage.value = ''
}

// ==================== 任务输入框（textarea 自动增高，类 AI 对话输入框） ====================

const inputRef = ref<HTMLTextAreaElement | null>(null)
// 与 CSS max-height 保持一致（默认 2 行，最多 10 行，超出滚动）
const INPUT_MAX_HEIGHT = 200

function resizeInput() {
  const el = inputRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, INPUT_MAX_HEIGHT)}px`
}

function onInputKeydown(e: KeyboardEvent) {
  // 回车提交（IME 组词中的回车不触发），Shift+回车换行
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    handleAdd()
  }
}

// 当前会话 id：宿主共享运行时的路由参数（仅终端窗口存在）
const sessionId = computed(() => {
  const shared = (window as any).__BEDCODE_SHARED__
  const id = shared?.router?.currentRoute?.value?.params?.id
  return typeof id === 'string' ? id : ''
})

// ==================== Status Display ====================

const statusLabel: Record<string, string> = {
  idle: t('idle'),
  in_progress: t('inProgress'),
  asking: t('asking'),
  completed: t('completed'),
  interrupted: t('interrupted'),
  pending: t('pending'),
}

const statusColor: Record<string, string> = {
  idle: 'var(--text-tertiary)',
  in_progress: 'var(--color-primary)',
  asking: '#f59e0b',
  completed: '#22c55e',
  interrupted: '#ef4444',
  pending: 'var(--text-tertiary)',
}

// 当前执行中的任务（执行中/等待输入），否则显示空闲
const displayTask = computed(() => {
  if (currentTask.value && ['in_progress', 'asking'].includes(currentTask.value.status)) {
    return currentTask.value
  }
  return null
})

// ==================== Data Loading ====================

async function loadQueue() {
  if (!sessionId.value) return
  try {
    const result: any = await context.commands.execute('auto-task.list-task-queue', {
      session_id: sessionId.value,
    })
    queue.value = (result?.tasks as QueueItem[]) || []
    activeTask.value = (result?.active_task as QueueItem | null) || null
  } catch (e) {
    console.error('[AutoTaskModal] Failed to load queue:', e)
    showError(t('loadFailed'))
  }
}

async function loadCurrentTask() {
  if (!sessionId.value) return
  try {
    const result: any = await context.commands.execute('auto-task.list-task-history', {
      session_id: sessionId.value,
    })
    const rows = (result?.tasks as HistoryRecord[]) || []
    currentTask.value = rows[0] || null
  } catch (e) {
    console.error('[AutoTaskModal] Failed to load current task:', e)
    showError(t('loadFailed'))
  }
}

// 预设任务：全局列表（无会话/未选会话时在侧边栏创建）
async function loadPresets() {
  try {
    const result: any = await context.commands.execute('auto-task.list-preset-tasks')
    presets.value = (result?.presets as PresetItem[]) || []
  } catch (e) {
    console.error('[AutoTaskModal] Failed to load presets:', e)
    showError(t('loadFailed'))
  }
}

// 把预设任务加入当前会话队列（一次性消耗，加入后预设自动移除）
async function addPreset(presetId: string) {
  if (!sessionId.value || addingPresetId.value) return
  addingPresetId.value = presetId
  try {
    await context.commands.execute('auto-task.add-preset-to-queue', {
      session_id: sessionId.value,
      preset_id: presetId,
    })
    // 事件广播（queue/preset-changed）兜底，此处直接刷新即时反馈
    await Promise.all([loadPresets(), loadQueue()])
  } catch (e) {
    console.error('[AutoTaskModal] Failed to add preset to queue:', e)
    showError(t('addPresetFailed'), e)
  } finally {
    addingPresetId.value = null
  }
}

// 会话开关：auto_execute（自动执行）/ auto_answer（自动应答）
async function loadSessionSettings() {
  if (!sessionId.value) return
  try {
    const result: any = await context.commands.execute('auto-task.get-session-settings', {
      session_id: sessionId.value,
    })
    autoExecute.value = result?.auto_execute === true
    autoAnswer.value = result?.auto_answer === true
  } catch (e) {
    console.error('[AutoTaskModal] Failed to load session settings:', e)
    showError(t('loadFailed'))
  }
}

async function refresh() {
  if (!sessionId.value) return
  loading.value = true
  try {
    await Promise.all([loadQueue(), loadCurrentTask(), loadSessionSettings(), loadPresets()])
  } finally {
    loading.value = false
  }
}

// ==================== Actions ====================

async function handleAdd() {
  const prompt = manualInput.value.trim()
  if (!sessionId.value || !prompt) return
  try {
    await context.commands.execute('auto-task.add-task', {
      session_id: sessionId.value,
      prompt,
    })
    manualInput.value = ''
    await nextTick()
    resizeInput()
    await loadQueue()
  } catch (e) {
    console.error('[AutoTaskModal] Failed to add task:', e)
    showError(t('addFailed'), e)
  }
}

async function handleRemove(taskId: string) {
  if (!sessionId.value) return
  try {
    await context.commands.execute('auto-task.remove-task', {
      session_id: sessionId.value,
      task_id: taskId,
    })
    await loadQueue()
  } catch (e) {
    console.error('[AutoTaskModal] Failed to remove task:', e)
    showError(t('removeFailed'), e)
  }
}

/** 取消活动队列项（waiting/executing）；预设状态由 cancel 广播经宿主转发落 interrupted */
async function handleCancelTask(taskId: string) {
  if (!sessionId.value) return
  try {
    await context.commands.execute('auto-task.cancel-task', {
      session_id: sessionId.value,
      task_id: taskId,
    })
    activeTask.value = null
    await loadQueue()
  } catch (e) {
    console.error('[AutoTaskModal] Failed to cancel task:', e)
    showError(t('cancelTaskFailed'), e)
  }
}

/** 点击清空：先进入内联确认态（替代 window.confirm） */
function handleClear() {
  if (!sessionId.value || queue.value.length === 0) return
  confirmingClear.value = true
}

async function confirmClear() {
  confirmingClear.value = false
  try {
    await context.commands.execute('auto-task.clear-queue', {
      session_id: sessionId.value,
    })
    queue.value = []
  } catch (e) {
    console.error('[AutoTaskModal] Failed to clear queue:', e)
    showError(t('clearFailed'), e)
  }
}

function cancelClear() {
  confirmingClear.value = false
}

/** 上移/下移：交换相邻元素后按新顺序调用 reorder-queue */
async function handleMove(index: number, direction: -1 | 1) {
  const target = index + direction
  if (target < 0 || target >= queue.value.length) return
  const items = [...queue.value]
  const [item] = items.splice(index, 1)
  items.splice(target, 0, item)
  await commitReorder(items)
}

async function commitReorder(items: QueueItem[]) {
  if (!sessionId.value) return
  try {
    await context.commands.execute('auto-task.reorder-queue', {
      session_id: sessionId.value,
      task_ids: items.map((i) => i.id),
    })
    // 本地立即更新，等待广播事件兜底
    queue.value = items.map((item, idx) => ({ ...item, position: idx }))
  } catch (e) {
    console.error('[AutoTaskModal] Failed to reorder queue:', e)
    showError(t('reorderFailed'), e)
    await loadQueue()
  }
}

function startEdit(item: QueueItem) {
  editingId.value = item.id
  editingText.value = item.prompt
}

async function saveEdit() {
  const prompt = editingText.value.trim()
  if (!sessionId.value || !editingId.value || !prompt) {
    editingId.value = null
    return
  }
  try {
    await context.commands.execute('auto-task.update-task', {
      session_id: sessionId.value,
      task_id: editingId.value,
      prompt,
    })
    editingId.value = null
    await loadQueue()
  } catch (e) {
    console.error('[AutoTaskModal] Failed to update task:', e)
    showError(t('updateFailed'), e)
  }
}

async function toggleAutoExecute() {
  if (!sessionId.value) return
  // 目标值必须在 await 前固化：后端 set_auto_mode 执行期间会同步发出
  // session:mode-changed 事件，若事件先于 invoke 返回到达，autoExecute 已被
  // 更新为 target，再用 !autoExecute.value 回写会把开关翻回旧值（开关看起来无变化）
  const target = !autoExecute.value
  try {
    await context.commands.execute('auto-task.set-auto-mode', {
      session_id: sessionId.value,
      auto_execute: target,
    })
    // 事件 session:mode-changed 会同步状态，此处按 target 幂等回写，避免闪烁
    autoExecute.value = target
  } catch (e) {
    console.error('[AutoTaskModal] Failed to toggle auto execute:', e)
    showError(t('modeFailed'), e)
  }
}

async function toggleAutoAnswer() {
  if (!sessionId.value) return
  // 与 toggleAutoExecute 同理：目标值提前固化，避免事件与本地回写竞争翻转开关
  const target = !autoAnswer.value
  try {
    await context.commands.execute('auto-task.set-auto-mode', {
      session_id: sessionId.value,
      auto_answer: target,
    })
    // 事件 session:mode-changed 会同步状态，此处按 target 幂等回写，避免闪烁
    autoAnswer.value = target
  } catch (e) {
    console.error('[AutoTaskModal] Failed to toggle auto answer:', e)
    showError(t('modeFailed'), e)
  }
}

// ==================== Events ====================

function onQueueChanged() {
  if (visible.value) loadQueue()
}

function onPresetChanged() {
  if (visible.value) loadPresets()
}

function onStatusChanged() {
  if (visible.value) loadCurrentTask()
}

function onModeChanged(data: any) {
  if (!visible.value || !data || data.session_id !== sessionId.value) return
  // 两个开关独立同步：autoExecute / autoAnswer（兼容旧字段 autoApprove）
  if (typeof data.autoExecute === 'boolean' || typeof data.auto_execute === 'boolean') {
    autoExecute.value = data.autoExecute === true || data.auto_execute === true
  }
  if (
    typeof data.autoAnswer === 'boolean' ||
    typeof data.auto_answer === 'boolean' ||
    typeof data.autoApprove === 'boolean' ||
    typeof data.auto_approve === 'boolean'
  ) {
    autoAnswer.value =
      data.autoAnswer === true || data.auto_answer === true || data.autoApprove === true || data.auto_approve === true
  }
}

// ==================== Lifecycle ====================

let queueDisposable: { dispose(): void } | null = null
let statusDisposable: { dispose(): void } | null = null
let modeDisposable: { dispose(): void } | null = null
let presetDisposable: { dispose(): void } | null = null

// Esc 关闭弹窗
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && visible.value) {
    visible.value = false
  }
}

// 弹窗打开时加载数据
watch(visible, (val) => {
  if (val) refresh()
})

onMounted(() => {
  window.addEventListener('keydown', onKeydown)
  queueDisposable = context.events.on('task:queue-changed', onQueueChanged)
  statusDisposable = context.events.on('task:status-changed', onStatusChanged)
  modeDisposable = context.events.on('session:mode-changed', onModeChanged)
  presetDisposable = context.events.on('task:preset-changed', onPresetChanged)
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
  queueDisposable?.dispose()
  statusDisposable?.dispose()
  modeDisposable?.dispose()
  presetDisposable?.dispose()
})
</script>

<template>
  <div v-if="visible" class="at-overlay" @click.self="visible = false">
    <div class="at-modal">
      <!-- Header -->
      <div class="at-header">
        <h3 class="at-title">{{ t('title') }}</h3>
        <div class="at-header-actions">
          <button
            v-if="queue.length > 0 && !confirmingClear"
            class="at-btn at-btn-danger"
            @click="handleClear"
          >
            {{ t('clearQueue') }}
          </button>
          <button class="at-close" :title="t('close')" @click="visible = false">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      <!-- 前端错误提示（替代系统 alert） -->
      <div v-if="errorMessage" class="at-error" role="alert">
        <span class="at-error-icon">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10" />
            <path d="M12 8v4m0 4h.01" />
          </svg>
        </span>
        <span class="at-error-text">{{ errorMessage }}</span>
        <button class="at-error-close" :title="t('close')" @click="clearError">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- 清空确认（替代 window.confirm） -->
      <div v-if="confirmingClear" class="at-confirm" role="alertdialog">
        <span class="at-confirm-text">{{ t('clearConfirm') }}</span>
        <div class="at-confirm-actions">
          <button class="at-btn at-btn-danger" @click="confirmClear">{{ t('confirm') }}</button>
          <button class="at-btn" @click="cancelClear">{{ t('cancel') }}</button>
        </div>
      </div>

      <!-- 无会话提示 -->
      <div v-if="!sessionId" class="at-empty">
        <p class="at-empty-text">{{ t('noSession') }}</p>
      </div>

      <template v-else>
        <!-- 当前任务状态 -->
        <div v-if="displayTask" class="at-current">
          <span class="at-dot" :style="{ background: statusColor[displayTask.status] }"></span>
          <span class="at-current-status" :style="{ color: statusColor[displayTask.status] }">
            {{ statusLabel[displayTask.status] || displayTask.status }}
          </span>
          <p class="at-current-desc">{{ displayTask.description || '—' }}</p>
        </div>
        <div v-else class="at-current at-current-idle">
          <span class="at-dot" :style="{ background: statusColor.idle }"></span>
          <span class="at-current-status" :style="{ color: statusColor.idle }">{{ t('idle') }}</span>
        </div>

        <!-- 自动执行开关：控制入队任务是否自动调度执行 -->
        <div class="at-mode-row">
          <div>
            <p class="at-mode-label">{{ t('autoExecute') }}</p>
            <p class="at-mode-hint">{{ t('autoExecuteHint') }}</p>
          </div>
          <button
            role="switch"
            :aria-checked="autoExecute"
            class="at-toggle"
            :class="{ 'at-toggle-on': autoExecute }"
            @click="toggleAutoExecute"
          >
            <span class="at-toggle-dot"></span>
          </button>
        </div>

        <!-- 自动应答开关：控制 Agent 提问是否自动回答 -->
        <div class="at-mode-row">
          <div>
            <p class="at-mode-label">{{ t('autoAnswer') }}</p>
            <p class="at-mode-hint">{{ t('autoAnswerHint') }}</p>
          </div>
          <button
            role="switch"
            :aria-checked="autoAnswer"
            class="at-toggle"
            :class="{ 'at-toggle-on': autoAnswer }"
            @click="toggleAutoAnswer"
          >
            <span class="at-toggle-dot"></span>
          </button>
        </div>

        <!-- 添加任务 -->
        <div class="at-input-row">
          <textarea
            ref="inputRef"
            v-model="manualInput"
            class="at-input at-input-textarea"
            rows="2"
            :placeholder="t('inputPlaceholder')"
            @input="resizeInput"
            @keydown="onInputKeydown"
          ></textarea>
          <button class="at-btn at-btn-primary" :disabled="!manualInput.trim()" @click="handleAdd">
            {{ t('add') }}
          </button>
        </div>

        <!-- 预设任务：选择加入当前会话队列（加入后自动从预设中移除） -->
        <div v-if="presets.length > 0" class="at-presets">
          <p class="at-presets-title">{{ t('presetTitle') }} ({{ presets.length }})</p>
          <div class="at-presets-list">
            <div v-for="p in presets" :key="p.id" class="at-preset-item">
              <span class="at-preset-prompt">{{ p.prompt }}</span>
              <button
                class="at-icon-btn at-icon-btn-primary"
                :disabled="addingPresetId !== null"
                :title="t('addToQueue')"
                @click="addPreset(p.id)"
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12 5v14m0 0l6-6m-6 6l-6-6" />
                </svg>
              </button>
            </div>
          </div>
        </div>

        <!-- 活动任务（waiting/executing）：可取消，用户中途放弃长任务的唯一入口 -->
        <div v-if="activeTask" class="at-active-task">
          <span class="at-active-task-label">{{ t('activeTask') }}</span>
          <span class="at-active-task-prompt">{{ activeTask.prompt }}</span>
          <button
            v-if="['waiting', 'executing'].includes(activeTask.status)"
            class="at-icon-btn at-icon-btn-danger"
            :title="t('cancelTask')"
            @click="handleCancelTask(activeTask.id)"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- 队列列表 -->
        <div class="at-queue">
          <div v-if="loading" class="at-empty">
            <p class="at-empty-text">{{ t('loading') }}</p>
          </div>
          <div v-else-if="queue.length === 0" class="at-empty">
            <p class="at-empty-text">{{ t('emptyQueue') }}</p>
            <p class="at-empty-hint">{{ t('emptyHint') }}</p>
          </div>
          <div v-else class="at-queue-list">
            <div v-for="(item, index) in queue" :key="item.id" class="at-item">
              <span class="at-position">{{ item.position + 1 }}</span>
              <!-- 编辑模式 -->
              <input
                v-if="editingId === item.id"
                v-model="editingText"
                class="at-input at-item-edit-input"
                type="text"
                @keydown.enter="saveEdit"
                @keydown.esc="editingId = null"
              />
              <p v-else class="at-item-prompt">{{ item.prompt }}</p>
              <div class="at-item-actions">
                <button
                  v-if="editingId === item.id"
                  class="at-icon-btn at-icon-btn-primary"
                  :title="t('save')"
                  @click="saveEdit"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M5 13l4 4L19 7" />
                  </svg>
                </button>
                <button
                  v-if="editingId === item.id"
                  class="at-icon-btn"
                  :title="t('cancel')"
                  @click="editingId = null"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
                <template v-else>
                  <button
                    class="at-icon-btn"
                    :disabled="index === 0"
                    :title="t('moveUp')"
                    @click="handleMove(index, -1)"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 19V5m0 0l-6 6m6-6l6 6" />
                    </svg>
                  </button>
                  <button
                    class="at-icon-btn"
                    :disabled="index === queue.length - 1"
                    :title="t('moveDown')"
                    @click="handleMove(index, 1)"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 5v14m0 0l6-6m-6 6l-6-6" />
                    </svg>
                  </button>
                  <button class="at-icon-btn" :title="t('edit')" @click="startEdit(item)">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
                      <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
                    </svg>
                  </button>
                  <button class="at-icon-btn at-icon-btn-danger" :title="t('delete')" @click="handleRemove(item.id)">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M3 6h18m-2 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
                    </svg>
                  </button>
                </template>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

