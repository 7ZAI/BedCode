<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="flex items-center gap-3">
        <button
          class="flex-shrink-0 p-1 -ml-1 transition-colors active:opacity-80"
          style="color: var(--mobile-text-secondary)"
          @click="router.back()"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h1 class="flex-1 page-title">{{ t('mobile.toolbox.presetTasks') }}</h1>
      </div>
    </div>

    <!-- Task List -->
    <div class="flex-1 overflow-y-auto overflow-x-hidden px-4 pb-24">
      <!-- Empty state -->
      <div
        v-if="tasks.length === 0"
        class="group-card p-6 text-center"
      >
        <svg class="w-10 h-10 mx-auto mb-3" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
        </svg>
        <p class="group-row-sub mb-3">{{ t('mobile.toolbox.noTasks') }}</p>
        <button
          class="text-sm transition-colors active:opacity-80"
          style="color: var(--mobile-accent)"
          @click="openAddDialog"
        >
          {{ t('mobile.toolbox.addTask') }}
        </button>
      </div>

      <!-- Card list -->
      <div v-else class="group-card">
        <PresetTaskCard
          v-for="task in tasks"
          :key="task.id"
          :task="task"
          @tap="handleTaskTap(task)"
          @execute="handleTaskTap(task)"
          @edit="openEditDialog($event)"
          @delete="handleDeleteTask($event)"
        />
      </div>
    </div>

    <!-- Bottom Add Button -->
    <div class="flex-shrink-0 p-4">
      <button
        class="w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
        style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)"
        @click="openAddDialog"
      >
        {{ t('mobile.toolbox.addTask') }}
      </button>
    </div>

    <!-- Add/Edit Dialog -->
    <TaskEditDialog
      :visible="showDialog"
      :task="editingTask"
      :is-connected="isConnected"
      :project-dirs="projectDirs"
      :active-session-id="activeSessionId"
      :active-sessions="activeSessions"
      :session-configs="connection.sessionConfigs.value || []"
      @save="handleSaveTask"
      @close="closeDialog"
    />

    <!-- Session Picker Dialog -->
    <Teleport to="body">
      <Transition name="bottom-sheet">
        <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0" style="background: var(--mobile-overlay-heavy)" @click="showSessionPicker = false"></div>
          <div class="relative w-full max-w-[clamp(280px,384px,440px)] rounded-2xl p-6 shadow-xl modal-panel" style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border)">
            <h3 class="page-title text-lg mb-4">{{ t('mobile.toolbox.selectSession') }}</h3>

            <div v-if="activeSessions.length === 0" class="text-center py-4">
              <p class="group-row-sub">{{ t('mobile.toolbox.noActiveSessions') }}</p>
            </div>

            <div v-else class="space-y-2 max-h-60 overflow-y-auto">
              <button
                v-for="session in activeSessions"
                :key="session.id"
                class="w-full text-left px-4 py-3 rounded-xl transition-colors active:opacity-80"
                style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-group-border)"
                @click="confirmExecute(session.id)"
              >
                <p class="group-row-title">{{ session.name }}</p>
                <p class="group-row-sub font-mono mt-0.5">{{ session.id.slice(0, 8) }}</p>
              </button>
            </div>

            <button
              class="w-full mt-4 h-10 rounded-xl text-sm font-medium transition-colors active:opacity-80"
              style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
              @click="showSessionPicker = false"
            >
              {{ t('common.button.cancel') }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Confirm Execute Dialog -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0" style="background: var(--mobile-overlay-heavy)" @click="showConfirmDialog = false"></div>
          <div class="relative w-full max-w-[clamp(280px,384px,440px)] rounded-2xl p-6 shadow-xl modal-panel" style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border)">
            <h3 class="page-title text-lg mb-2">{{ t('mobile.toolbox.confirmExecute') }}</h3>
            <p class="text-sm mb-1" style="color: var(--mobile-text-muted)">{{ t('mobile.toolbox.willSendToTerminal') }}</p>
            <p class="text-sm rounded-lg p-3 mb-4 line-clamp-3" style="color: var(--mobile-row-title); background: var(--mobile-bg-primary)">{{ pendingTask?.content }}</p>

            <div class="flex gap-3">
              <button
                class="flex-1 h-10 rounded-xl text-sm font-medium transition-colors active:opacity-80"
                style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
                @click="showConfirmDialog = false"
              >
                {{ t('common.button.cancel') }}
              </button>
              <button
                class="flex-1 h-10 rounded-xl text-sm font-medium transition-colors active:opacity-80"
                style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)"
                @click="doExecute"
              >
                {{ t('mobile.toolbox.execute') }}
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * PresetTasksView - 预设任务二级页面
 *
 * 从工具箱入口进入：任务卡片列表、新增/编辑、选择会话执行、确认执行
 */

import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { useToast } from '@/composables/useToast'
import PresetTaskCard from '@/components/PresetTaskCard.vue'
import TaskEditDialog from '@/components/TaskEditDialog.vue'
import type { PresetTask } from '@/composables/model'

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()
const { t } = useI18n()
const { tasks, load, addTask, updateTask, deleteTask, executeTask } = usePresetTasks()

const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')
const activeSessionId = computed(() => connection.activeSessionId.value || '')
const activeSessions = computed(() => connection.activeSessions.value || [])

// ==================== 项目目录选择 ====================

/** 从会话配置中提取去重的工程目录列表 */
const projectDirs = computed(() => {
  const configs = connection.sessionConfigs.value || []
  const dirs = configs
    .map(c => c.working_dir)
    .filter((d): d is string => !!d)
  return [...new Set(dirs)]
})

// ==================== 任务编辑 ====================

const showDialog = ref(false)
const editingTask = ref<PresetTask | null>(null)

// Session picker & confirm
const showSessionPicker = ref(false)
const showConfirmDialog = ref(false)
const pendingTask = ref<PresetTask | null>(null)
const pendingSessionId = ref('')

onMounted(async () => {
  await load()
})

function openAddDialog() {
  editingTask.value = null
  showDialog.value = true
}

function openEditDialog(task: PresetTask) {
  editingTask.value = task
  showDialog.value = true
}

function closeDialog() {
  showDialog.value = false
  editingTask.value = null
}

/** TaskEditDialog 保存回调：区分新增/编辑 */
async function handleSaveTask(data: PresetTask | { content: string }) {
  if ('id' in data) {
    // 编辑模式：data 是完整 PresetTask
    await updateTask(data)
  } else {
    // 新增模式：data 是 { content }
    await addTask(data)
  }
  closeDialog()
}

async function handleDeleteTask(id: string) {
  await deleteTask(id)
}

/** 点击卡片/执行按钮 → session picker flow */
function handleTaskTap(task: PresetTask) {
  pendingTask.value = task

  if (!isConnected.value) {
    toast.warning(t('mobile.toolbox.connectFirst'))
    router.push({ name: 'mobile-home', query: { page: '0' } })
    return
  }

  const sessions = activeSessions.value

  // 没有活跃会话
  if (sessions.length === 0) {
    toast.warning(t('mobile.toolbox.noActiveSessions'))
    return
  }

  // 仅一个活跃会话时跳过 picker，直接确认
  if (sessions.length === 1) {
    pendingSessionId.value = sessions[0].id
    showConfirmDialog.value = true
    return
  }

  // 多个活跃会话时显示 picker，让用户选择目标会话
  showSessionPicker.value = true
}

/** Session picker 选择后 → 显示确认 */
function confirmExecute(sessionId: string) {
  showSessionPicker.value = false
  pendingSessionId.value = sessionId
  showConfirmDialog.value = true
}

/** 确认执行 */
async function doExecute() {
  if (!pendingTask.value || !pendingSessionId.value) return

  showConfirmDialog.value = false

  try {
    await executeTask(pendingTask.value, pendingSessionId.value)
    toast.success(t('mobile.toolbox.sentToTerminal'))
  } catch {
    toast.error(t('mobile.toolbox.sendFailed'))
  }

  pendingTask.value = null
  pendingSessionId.value = ''
}
</script>
