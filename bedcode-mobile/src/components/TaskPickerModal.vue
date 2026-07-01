<script setup lang="ts">
/**
 * TaskPickerModal - 任务选择弹窗
 *
 * 居中弹窗展示可选的 PresetTask 列表，支持勾选、排序和新建任务
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PresetTask, PresetTaskType } from '../composables/model'
import { usePresetTasks } from '../composables/usePresetTasks'

const { t } = useI18n()

const props = defineProps<{
  tasks: PresetTask[]
}>()

const emit = defineEmits<{
  confirm: [tasks: PresetTask[]]
  close: []
}>()

const { addTask } = usePresetTasks()

const selectedIds = ref<Set<string>>(new Set())
const orderedSelection = ref<PresetTask[]>([])
const expandedTaskId = ref<string | null>(null)

// 新建任务弹窗状态
const showCreateModal = ref(false)
const newTitle = ref('')
const newContent = ref('')
const newType = ref<PresetTaskType>('template')

const availableTasks = computed(() =>
  props.tasks.filter(t => t.type === 'template' || (t.type === 'once' && t.status === 'pending'))
)

function toggleTask(task: PresetTask) {
  if (selectedIds.value.has(task.id)) {
    selectedIds.value.delete(task.id)
    orderedSelection.value = orderedSelection.value.filter(t => t.id !== task.id)
  } else {
    selectedIds.value.add(task.id)
    orderedSelection.value.push(task)
  }
}

function toggleExpand(taskId: string) {
  expandedTaskId.value = expandedTaskId.value === taskId ? null : taskId
}

function moveUp(index: number) {
  if (index <= 0) return
  const list = [...orderedSelection.value]
  ;[list[index - 1], list[index]] = [list[index], list[index - 1]]
  orderedSelection.value = list
}

function moveDown(index: number) {
  if (index >= orderedSelection.value.length - 1) return
  const list = [...orderedSelection.value]
  ;[list[index], list[index + 1]] = [list[index + 1], list[index]]
  orderedSelection.value = list
}

function handleConfirm() {
  if (orderedSelection.value.length > 0) {
    emit('confirm', orderedSelection.value)
  }
}

/** 打开新建任务弹窗 */
function openCreateModal() {
  newTitle.value = ''
  newContent.value = ''
  newType.value = 'template'
  showCreateModal.value = true
}

/** 确认创建任务 */
async function handleCreate() {
  const title = newTitle.value.trim()
  const content = newContent.value.trim()
  if (!title || !content) return

  await addTask({ title, content, type: newType.value })
  showCreateModal.value = false
}
</script>

<template>
  <Teleport to="body">
    <div class="modal-overlay mobile-ui" @click.self="emit('close')">
      <div class="modal-content">
        <div class="modal-header">
          <h3>{{ t('mobile.taskPicker.title') }}</h3>
          <div class="header-actions">
            <button class="add-btn" @click="openCreateModal" :title="t('mobile.taskPicker.newTask')">
              <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
                <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z"/>
              </svg>
            </button>
            <button class="close-btn" @click="emit('close')">&times;</button>
          </div>
        </div>

        <div class="modal-body">
          <!-- 可选任务列表 -->
          <div v-if="availableTasks.length === 0" class="empty-hint">
            <p>{{ t('mobile.taskPicker.noTasks') }}</p>
            <button class="empty-add-btn" @click="openCreateModal">{{ t('mobile.taskPicker.createTask') }}</button>
          </div>
          <div v-else class="task-list">
            <div
              v-for="task in availableTasks"
              :key="task.id"
              class="task-item"
              :class="{ selected: selectedIds.has(task.id), expanded: expandedTaskId === task.id }"
            >
              <div class="task-item-main" @click="toggleTask(task)">
                <div class="task-checkbox">
                  <svg v-if="selectedIds.has(task.id)" viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
                    <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
                  </svg>
                </div>
                <div class="task-info">
                  <div class="task-info-top">
                    <span class="task-title">{{ task.title }}</span>
                    <span class="task-type-badge">{{ task.type === 'template' ? t('mobile.presetTask.template') : t('mobile.presetTask.once') }}</span>
                  </div>
                  <span class="task-content-preview">{{ task.content }}</span>
                </div>
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

          <!-- 已选任务排序 -->
          <div v-if="orderedSelection.length > 0" class="selected-section">
            <div class="section-title">{{ t('mobile.taskPicker.executionOrder') }}</div>
            <div class="selected-list">
              <div v-for="(task, index) in orderedSelection" :key="task.id" class="selected-item">
                <span class="order-number">{{ index + 1 }}</span>
                <div class="selected-info">
                  <span class="selected-title">{{ task.title }}</span>
                  <span class="selected-content-preview">{{ task.content }}</span>
                </div>
                <div class="order-actions">
                  <button class="order-btn" :disabled="index === 0" @click.stop="moveUp(index)">↑</button>
                  <button class="order-btn" :disabled="index === orderedSelection.length - 1" @click.stop="moveDown(index)">↓</button>
                  <button class="order-btn remove" @click.stop="toggleTask(task)">&times;</button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <button class="btn-cancel" @click="emit('close')">{{ t('common.button.cancel') }}</button>
          <button
            class="btn-confirm"
            :disabled="orderedSelection.length === 0"
            @click="handleConfirm"
          >
            {{ t('mobile.taskPicker.confirmAdd', { count: orderedSelection.length }) }}
          </button>
        </div>
      </div>
    </div>

    <!-- 新建任务弹窗 -->
    <div v-if="showCreateModal" class="create-overlay mobile-ui" @click.self="showCreateModal = false">
      <div class="create-modal">
        <div class="create-header">
          <h3>{{ t('mobile.taskPicker.newTask') }}</h3>
          <button class="close-btn" @click="showCreateModal = false">&times;</button>
        </div>

        <div class="create-body">
          <div class="form-group">
            <label class="form-label">{{ t('mobile.taskPicker.taskName') }}</label>
            <input
              v-model="newTitle"
              class="form-input"
              :placeholder="t('mobile.taskPicker.taskNamePlaceholder')"
              maxlength="50"
            />
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('mobile.taskPicker.taskContent') }}</label>
            <textarea
              v-model="newContent"
              class="form-textarea"
              :placeholder="t('mobile.taskPicker.taskContentPlaceholder')"
              rows="4"
            ></textarea>
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('mobile.taskPicker.taskType') }}</label>
            <div class="type-selector">
              <button
                class="type-btn"
                :class="{ active: newType === 'template' }"
                @click="newType = 'template'"
              >
                {{ t('mobile.presetTask.template') }}
              </button>
              <button
                class="type-btn"
                :class="{ active: newType === 'once' }"
                @click="newType = 'once'"
              >
                {{ t('mobile.presetTask.once') }}
              </button>
            </div>
            <p class="type-hint">{{ newType === 'template' ? t('mobile.taskPicker.templateHint') : t('mobile.taskPicker.onceHint') }}</p>
          </div>
        </div>

        <div class="create-footer">
          <button class="btn-cancel" @click="showCreateModal = false">{{ t('common.button.cancel') }}</button>
          <button
            class="btn-confirm"
            :disabled="!newTitle.trim() || !newContent.trim()"
            @click="handleCreate"
          >
            {{ t('common.button.create') }}
          </button>
        </div>
      </div>
    </div>
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

.task-item.selected {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
}

.task-item-main {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  cursor: pointer;
}

.task-item-main:active {
  background: var(--mobile-bg-hover);
}

.task-checkbox {
  width: 20px;
  height: 20px;
  border-radius: 4px;
  border: 2px solid var(--mobile-border);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--mobile-accent);
  flex-shrink: 0;
}

.task-item.selected .task-checkbox {
  border-color: var(--mobile-accent);
}

.task-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  overflow: hidden;
  flex: 1;
  min-width: 0;
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

.selected-section {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--mobile-border);
}

.section-title {
  font-size: 12px;
  color: var(--mobile-text-muted);
  margin-bottom: 8px;
}

.selected-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.selected-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--mobile-bg-secondary);
}

.selected-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow: hidden;
}

.order-number {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 600;
  flex-shrink: 0;
}

.selected-title {
  font-size: 13px;
  color: var(--mobile-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.selected-content-preview {
  font-size: 11px;
  color: var(--mobile-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.order-actions {
  display: flex;
  gap: 4px;
}

.order-btn {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 4px;
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-muted);
  font-size: 14px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
}

.order-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.order-btn.remove {
  color: var(--mobile-error, #ef4444);
}

.modal-footer {
  display: flex;
  gap: 12px;
  padding: 12px 16px;
  border-top: 1px solid var(--mobile-border);
}

.btn-cancel {
  flex: 1;
  padding: 10px;
  border: 1px solid var(--mobile-border);
  border-radius: 8px;
  background: transparent;
  color: var(--mobile-text-secondary);
  font-size: 14px;
  cursor: pointer;
}

.btn-confirm {
  flex: 1;
  padding: 10px;
  border: none;
  border-radius: 8px;
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}

.btn-confirm:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 新建任务弹窗 */
.create-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--mobile-overlay);
  padding: 1rem;
}

.create-modal {
  width: 100%;
  max-width: 380px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  background: var(--mobile-bg-secondary);
  border-radius: 16px;
  overflow: hidden;
  animation: modal-in 0.2s ease;
}

.create-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px;
  border-bottom: 1px solid var(--mobile-border);
}

.create-header h3 {
  margin: 0;
  font-size: 16px;
  color: var(--mobile-text-primary);
}

.create-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

.form-group {
  margin-bottom: 16px;
}

.form-group:last-child {
  margin-bottom: 0;
}

.form-label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  color: var(--mobile-text-muted);
  margin-bottom: 6px;
}

.form-input {
  width: 100%;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-size: 14px;
  outline: none;
  transition: border-color 0.15s;
  box-sizing: border-box;
}

.form-input:focus {
  border-color: var(--mobile-accent);
}

.form-input::placeholder {
  color: var(--mobile-text-muted);
}

.form-textarea {
  width: 100%;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-size: 14px;
  outline: none;
  resize: vertical;
  min-height: 80px;
  transition: border-color 0.15s;
  box-sizing: border-box;
  font-family: inherit;
}

.form-textarea:focus {
  border-color: var(--mobile-accent);
}

.form-textarea::placeholder {
  color: var(--mobile-text-muted);
}

.type-selector {
  display: flex;
  gap: 8px;
}

.type-btn {
  flex: 1;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  font-size: 14px;
  cursor: pointer;
  transition: all 0.15s;
}

.type-btn.active {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  font-weight: 600;
}

.type-hint {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--mobile-text-muted);
}

.create-footer {
  display: flex;
  gap: 12px;
  padding: 12px 16px;
  border-top: 1px solid var(--mobile-border);
}
</style>
