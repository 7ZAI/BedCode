<script setup lang="ts">
/**
 * TaskPickerModal - 任务选择弹窗
 *
 * 展示可选的 PresetTask 列表，支持勾选和排序
 */
import { ref, computed } from 'vue'
import type { PresetTask } from '../composables/model'

const props = defineProps<{
  tasks: PresetTask[]
}>()

const emit = defineEmits<{
  confirm: [tasks: PresetTask[]]
  close: []
}>()

const selectedIds = ref<Set<string>>(new Set())
const orderedSelection = ref<PresetTask[]>([])

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
</script>

<template>
  <Teleport to="body">
    <div class="modal-overlay" @click.self="emit('close')">
      <div class="modal-content">
        <div class="modal-header">
          <h3>选择待办任务</h3>
          <button class="close-btn" @click="emit('close')">&times;</button>
        </div>

        <div class="modal-body">
          <!-- 可选任务列表 -->
          <div v-if="availableTasks.length === 0" class="empty-hint">
            暂无可选任务，请先在工具箱中创建
          </div>
          <div v-else class="task-list">
            <div
              v-for="task in availableTasks"
              :key="task.id"
              class="task-item"
              :class="{ selected: selectedIds.has(task.id) }"
              @click="toggleTask(task)"
            >
              <div class="task-checkbox">
                <svg v-if="selectedIds.has(task.id)" viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
                  <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
                </svg>
              </div>
              <div class="task-info">
                <span class="task-title">{{ task.title }}</span>
                <span class="task-type-badge">{{ task.type === 'template' ? '模板' : '一次性' }}</span>
              </div>
            </div>
          </div>

          <!-- 已选任务排序 -->
          <div v-if="orderedSelection.length > 0" class="selected-section">
            <div class="section-title">执行顺序</div>
            <div class="selected-list">
              <div v-for="(task, index) in orderedSelection" :key="task.id" class="selected-item">
                <span class="order-number">{{ index + 1 }}</span>
                <span class="selected-title">{{ task.title }}</span>
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
          <button class="btn-cancel" @click="emit('close')">取消</button>
          <button
            class="btn-confirm"
            :disabled="orderedSelection.length === 0"
            @click="handleConfirm"
          >
            确认添加 ({{ orderedSelection.length }})
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
}

.modal-content {
  width: 100%;
  max-height: 70vh;
  display: flex;
  flex-direction: column;
  background: var(--mobile-bg-secondary);
  border-radius: 16px 16px 0 0;
  overflow: hidden;
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  border-bottom: 1px solid var(--mobile-border);
}

.modal-header h3 {
  margin: 0;
  font-size: 16px;
  color: var(--mobile-text-primary);
}

.close-btn {
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  font-size: 24px;
  cursor: pointer;
  padding: 0 4px;
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

.task-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.task-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  cursor: pointer;
  transition: background 0.15s;
}

.task-item:active {
  background: var(--mobile-bg-hover);
}

.task-item.selected {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
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
  align-items: center;
  gap: 8px;
  overflow: hidden;
}

.task-title {
  font-size: 14px;
  color: var(--mobile-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.order-number {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--mobile-accent);
  color: #0a0a0f;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 600;
  flex-shrink: 0;
}

.selected-title {
  flex: 1;
  font-size: 13px;
  color: var(--mobile-text-primary);
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
  color: #0a0a0f;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}

.btn-confirm:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
