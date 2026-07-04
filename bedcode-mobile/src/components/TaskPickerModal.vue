<script setup lang="ts">
/**
 * TaskPickerModal - 可执行任务弹窗
 *
 * 展示所有预设任务，每个任务可单独点击执行发送到终端，
 * 也可新建/编辑任务
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PresetTask } from '../composables/model'
import { usePresetTasks } from '../composables/usePresetTasks'
import { useMobileConnection } from '@/composables/useMobileConnection'
import TaskEditDialog from '@/components/TaskEditDialog.vue'

const { t } = useI18n()
const connection = useMobileConnection()

const props = defineProps<{
  tasks: PresetTask[]
  /**
   * 当前终端会话 ID（终端视图场景传入）
   * 传入时：新建/编辑任务弹窗自动锁定该会话的工作目录，不可更改
   * 不传时：目录可自由选择（工具箱场景）
   */
  sessionId?: string
}>()

const emit = defineEmits<{
  /** 执行单个任务：将任务内容发送到终端 */
  execute: [task: PresetTask]
  close: []
}>()

const { addTask, updateTask } = usePresetTasks()

const expandedTaskId = ref<string | null>(null)

// TaskEditDialog 状态
const showEditDialog = ref(false)
const editingTask = ref<PresetTask | null>(null)

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)
const activeSessionId = computed(() => connection.activeSessionId.value || '')
const activeSessions = computed(() => connection.activeSessions.value || [])
const projectDirs = computed(() => {
  const configs = connection.sessionConfigs.value || []
  const dirs = configs
    .map((c: any) => c.working_dir)
    .filter((d: any): d is string => !!d)
  return [...new Set(dirs)]
})

/** 根据 sessionId 查找对应会话的工作目录，用于锁定 TaskEditDialog */
const lockedDir = computed(() => {
  if (!props.sessionId) return undefined
  const sessions = activeSessions.value
  const session = sessions.find((s: any) => s.id === props.sessionId)
  if (!session) return undefined
  const configId = session.config_id || session.configId
  if (!configId) return undefined
  const configs = connection.sessionConfigs.value || []
  const config = configs.find((c: any) => c.id === configId)
  return config?.working_dir || undefined
})

function toggleExpand(taskId: string) {
  expandedTaskId.value = expandedTaskId.value === taskId ? null : taskId
}

/** 点击执行按钮 */
function handleExecute(task: PresetTask) {
  emit('execute', task)
}

/** 打开新建任务弹窗 */
function openCreateDialog() {
  editingTask.value = null
  showEditDialog.value = true
}

/** 打开编辑任务弹窗 */
function openEditDialog(task: PresetTask) {
  editingTask.value = task
  showEditDialog.value = true
}

/** TaskEditDialog 保存回调 */
async function handleEditSave(data: PresetTask | { title: string; content: string }) {
  if ('id' in data) {
    await updateTask(data)
  } else {
    await addTask(data)
  }
  showEditDialog.value = false
  editingTask.value = null
}
</script>

<template>
  <Teleport to="body">
    <div class="modal-overlay mobile-ui" @click.self="emit('close')">
      <div class="modal-content">
        <div class="modal-header">
          <h3>{{ t('mobile.taskPicker.title') }}</h3>
          <div class="header-actions">
            <button class="add-btn" @click="openCreateDialog" :title="t('mobile.taskPicker.newTask')">
              <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
                <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z"/>
              </svg>
            </button>
            <button class="close-btn" @click="emit('close')">&times;</button>
          </div>
        </div>

        <div class="modal-body">
          <!-- 任务列表 -->
          <div v-if="tasks.length === 0" class="empty-hint">
            <p>{{ t('mobile.taskPicker.noTasks') }}</p>
            <button class="empty-add-btn" @click="openCreateDialog">{{ t('mobile.taskPicker.createTask') }}</button>
          </div>
          <div v-else class="task-list">
            <div
              v-for="task in tasks"
              :key="task.id"
              class="task-item"
              :class="{ expanded: expandedTaskId === task.id }"
            >
              <div class="task-item-main">
                <div class="task-info" @click="toggleExpand(task.id)">
                  <div class="task-info-top">
                    <span class="task-title">{{ task.title }}</span>
                    <span class="task-type-badge">{{ task.type === 'template' ? t('mobile.presetTask.template') : t('mobile.presetTask.once') }}</span>
                  </div>
                  <span class="task-content-preview">{{ task.content }}</span>
                </div>
                <!-- 执行按钮 -->
                <button class="exec-btn" :title="t('mobile.presetTask.execute')" @click.stop="handleExecute(task)">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
                    <path d="M8 5v14l11-7z"/>
                  </svg>
                </button>
                <!-- 编辑按钮 -->
                <button class="edit-btn" :title="t('mobile.presetTask.edit')" @click.stop="openEditDialog(task)">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/>
                  </svg>
                </button>
                <!-- 展开/收起按钮 -->
                <button class="expand-btn" :class="{ rotated: expandedTaskId === task.id }" @click.stop="toggleExpand(task.id)">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
                    <path d="M7 10l5 5 5-5z"/>
                  </svg>
                </button>
              </div>
              <transition name="content-expand">
                <div v-if="expandedTaskId === task.id" class="task-content-full">
                  <pre class="task-content-text">{{ task.content }}</pre>
                </div>
              </transition>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 新增/编辑任务弹窗（使用共享组件） -->
    <TaskEditDialog
      :visible="showEditDialog"
      :task="editingTask"
      :is-connected="isConnected"
      :project-dirs="projectDirs"
      :active-session-id="activeSessionId"
      :active-sessions="activeSessions"
      :session-configs="connection.sessionConfigs.value || []"
      :locked-dir="lockedDir"
      @save="handleEditSave"
      @close="showEditDialog = false"
    />
  </Teleport>
</template>

<style scoped>
/* 主弹窗遮罩 - 居中 */
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--mobile-overlay);
  padding: 1rem;
}

.modal-content {
  width: 100%;
  max-width: 380px;
  max-height: 75vh;
  display: flex;
  flex-direction: column;
  background: var(--mobile-bg-secondary);
  border-radius: 16px;
  overflow: hidden;
  animation: modal-in 0.2s ease;
}

@keyframes modal-in {
  from {
    opacity: 0;
    transform: scale(0.95);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px;
  border-bottom: 1px solid var(--mobile-border);
}

.modal-header h3 {
  margin: 0;
  font-size: 16px;
  color: var(--mobile-text-primary);
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.add-btn {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.15s;
}

.add-btn:active {
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
}

.close-btn {
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  font-size: 24px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
}

.close-btn:hover {
  color: var(--mobile-text-primary);
}

.modal-body {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
}

.empty-hint {
  text-align: center;
  color: var(--mobile-text-muted);
  padding: 24px 0;
  font-size: 14px;
}

.empty-add-btn {
  margin-top: 12px;
  padding: 8px 20px;
  border-radius: 8px;
  border: 1px dashed var(--mobile-accent);
  background: transparent;
  color: var(--mobile-accent);
  font-size: 14px;
  cursor: pointer;
  transition: background 0.15s;
}

.empty-add-btn:active {
  background: var(--mobile-accent-muted);
}

.task-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.task-item {
  display: flex;
  flex-direction: column;
  border-radius: 8px;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  overflow: hidden;
  transition: border-color 0.15s, background 0.15s;
}

.task-item-main {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 10px 12px;
}

.task-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  overflow: hidden;
  flex: 1;
  min-width: 0;
  cursor: pointer;
}

.task-info:active {
  opacity: 0.8;
}

.task-info-top {
  display: flex;
  align-items: center;
  gap: 8px;
}

.task-title {
  font-size: 14px;
  color: var(--mobile-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-content-preview {
  font-size: 12px;
  color: var(--mobile-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.exec-btn {
  width: 32px;
  height: 32px;
  border-radius: 6px;
  border: none;
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: background 0.15s, color 0.15s;
}

.exec-btn:active {
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
}

.edit-btn {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: color 0.15s, background 0.15s;
}

.edit-btn:active {
  background: var(--mobile-bg-hover);
  color: var(--mobile-accent);
}

.expand-btn {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: transform 0.2s ease, color 0.15s;
}

.expand-btn:active {
  background: var(--mobile-bg-hover);
}

.expand-btn.rotated {
  transform: rotate(180deg);
}

.task-content-full {
  padding: 0 12px 10px;
}

.task-content-text {
  margin: 0;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--mobile-bg-secondary);
  color: var(--mobile-text-secondary);
  font-size: 12px;
  font-family: 'Courier New', Courier, monospace;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
}

/* 展开动画 */
.content-expand-enter-active,
.content-expand-leave-active {
  transition: all 0.2s ease;
  overflow: hidden;
}

.content-expand-enter-from,
.content-expand-leave-to {
  opacity: 0;
  max-height: 0;
  padding-top: 0;
  padding-bottom: 0;
}

.content-expand-enter-to,
.content-expand-leave-from {
  opacity: 1;
  max-height: 220px;
}

.task-type-badge {
  flex-shrink: 0;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--mobile-bg-secondary);
  color: var(--mobile-text-muted);
}
</style>
