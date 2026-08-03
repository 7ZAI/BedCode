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
              <button v-if="queue.length > 0" class="clear-btn" @click="handleClear">{{ $t('mobile.autoTask.clear') }}</button>
              <button class="close-btn" @click="$emit('close')">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>

          <!-- Preset Task Selection -->
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
              <div v-for="task in queue" :key="task.id" class="queue-item">
                <div class="queue-item-main">
                  <span class="queue-item-position">{{ task.position + 1 }}</span>
                  <p class="queue-item-prompt">{{ task.prompt }}</p>
                </div>
                <button class="queue-item-delete" @click="handleRemove(task.id)">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
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
 * 使用宿主 composable 直接访问 HTTP API 和连接状态
 */
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { httpTaskQueueList, httpTaskQueueAdd, httpTaskQueueRemove, httpTaskQueueClear } from '@/composables/useHttpApi'
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

// 面板打开时加载队列
watch(() => props.visible, (val) => {
  if (val && props.activeSessionId) {
    loadQueue()
  }
})

async function loadQueue() {
  if (!props.activeSessionId) return
  loading.value = true
  try {
    const result = await httpTaskQueueList(props.activeSessionId)
    if (result.code === 0 && result.data) {
      queue.value = (result.data as QueueListResponse).tasks || []
    }
  } catch (e) {
    console.error('[AutoTask] Failed to load queue:', e)
    toast.error(t('mobile.autoTask.loadFailed'))
  } finally {
    loading.value = false
  }
}

async function handleAddFromPreset(task: any) {
  if (!props.activeSessionId) return
  const result = await httpTaskQueueAdd(props.activeSessionId, task.content)
  if (result.code === 0) {
    await loadQueue()
  } else {
    toast.error(t('mobile.autoTask.addFailed'))
  }
}

async function handleAddManual() {
  if (!props.activeSessionId || !manualInput.value.trim()) return
  const result = await httpTaskQueueAdd(props.activeSessionId, manualInput.value.trim())
  if (result.code === 0) {
    manualInput.value = ''
    await loadQueue()
  } else {
    toast.error(t('mobile.autoTask.addFailed'))
  }
}

async function handleRemove(taskId: string) {
  if (!props.activeSessionId) return
  const result = await httpTaskQueueRemove(props.activeSessionId, taskId)
  if (result.code === 0) {
    await loadQueue()
  } else {
    toast.error(t('mobile.autoTask.removeFailed'))
  }
}

async function handleClear() {
  if (!props.activeSessionId) return
  const result = await httpTaskQueueClear(props.activeSessionId)
  if (result.code === 0) {
    queue.value = []
  } else {
    toast.error(t('mobile.autoTask.clearFailed'))
  }
}
</script>

<style scoped>
.panel {
  position: relative;
  width: 100%;
  max-height: 70vh;
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

.preset-section {
  padding: 0 1.25rem 0.75rem;
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

.queue-item:active {
  transform: scale(0.98);
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

.queue-item-delete {
  flex-shrink: 0;
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

.queue-item-delete:hover {
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
