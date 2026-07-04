<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center justify-between gap-2">
    
    </header>

    <!-- Main Content Area -->
    <div class="flex-1 overflow-hidden relative">
      <!-- Task List -->
      <div class="h-full overflow-y-auto p-4 space-y-5">

        <!-- Section: 预设任务 -->
        <section>
          <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">{{ t('mobile.toolbox.presetTasks') }}</h3>

          <!-- Empty state -->
          <div
            v-if="tasks.length === 0"
            class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-6 text-center shadow-[var(--mobile-card-shadow)]"
          >
            <svg class="w-10 h-10 mx-auto mb-3 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
            </svg>
            <p class="text-[var(--mobile-text-disabled)] text-sm mb-3">{{ t('mobile.toolbox.noTasks') }}</p>
            <button
              class="text-sm text-[var(--mobile-accent)] hover:text-cyan-300 active:opacity-80 transition-colors"
              @click="openAddDialog"
            >
              {{ t('mobile.toolbox.addTask') }}
            </button>
          </div>

          <!-- Card list -->
          <div v-else class="space-y-2.5">
            <PresetTaskCard
              v-for="task in tasks"
              :key="task.id"
              :task="task"
              @tap="handleTaskTap(task)"
              @execute="handleTaskExecute(task)"
              @edit="openEditDialog($event)"
              @delete="handleDeleteTask($event)"
            />
          </div>
        </section>

        <!-- Section: 插件（预留 - 暂时注释） -->
        <!-- <section>
          <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">{{ t('mobile.toolbox.plugins') }}</h3>
          <div class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 text-center">
            <p class="text-[var(--mobile-text-disabled)] text-sm">{{ t('mobile.toolbox.comingSoon') }}</p>
          </div>
        </section> -->

      </div>

      <!-- Bottom Add Button -->
      <div class="absolute bottom-0 left-0 right-0 p-4 bg-gradient-to-t from-[var(--mobile-bg-primary)] via-[var(--mobile-bg-primary)]/90 to-transparent pointer-events-none">
        <button
          class="w-full py-3 bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] rounded-xl font-medium hover:bg-[var(--mobile-accent)]/30 active:scale-[0.98] transition-all duration-150 flex items-center justify-center gap-2 pointer-events-auto"
          @click="openAddDialog"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          {{ t('mobile.toolbox.addTask') }}
        </button>
      </div>

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
          <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showSessionPicker = false"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl modal-panel">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4">{{ t('mobile.toolbox.selectSession') }}</h3>

            <div v-if="activeSessions.length === 0" class="text-center py-4">
              <p class="text-[var(--mobile-text-disabled)] text-sm">{{ t('mobile.toolbox.noActiveSessions') }}</p>
            </div>

            <div v-else class="space-y-2 max-h-60 overflow-y-auto">
              <button
                v-for="session in activeSessions"
                :key="session.id"
                class="w-full text-left px-4 py-3 rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-primary)] hover:border-[var(--mobile-accent)]/30 active:opacity-80 transition-colors"
                @click="confirmExecute(session.id)"
              >
                <p class="text-sm font-medium text-[var(--mobile-text-primary)]">{{ session.name }}</p>
                <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5">{{ session.id.slice(0, 8) }}</p>
              </button>
            </div>

            <button
              class="w-full mt-4 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-[var(--mobile-accent)]/40 active:opacity-80 transition-colors"
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
          <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showConfirmDialog = false"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl modal-panel">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-2">{{ t('mobile.toolbox.confirmExecute') }}</h3>
            <p class="text-sm text-[var(--mobile-text-muted)] mb-1">{{ t('mobile.toolbox.willSendToTerminal') }}</p>
            <p class="text-sm text-[var(--mobile-text-primary)] bg-[var(--mobile-bg-primary)] rounded-lg p-3 mb-4 line-clamp-3">{{ pendingTask?.content }}</p>

            <div class="flex gap-3">
              <button
                class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-[var(--mobile-accent)]/40 active:opacity-80 transition-colors"
                @click="showConfirmDialog = false"
              >
                {{ t('common.button.cancel') }}
              </button>
              <button
                class="flex-1 bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] py-2.5 rounded-xl font-medium hover:bg-[var(--mobile-accent)]/30 active:scale-[0.98] transition-all duration-150"
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
 * ToolboxView - 工具箱页面
 *
 * 预设任务管理 + 文件浏览侧栏
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

// ==================== 预设任务 ====================

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
async function handleSaveTask(data: PresetTask | { title: string; content: string }) {
  if ('id' in data) {
    // 编辑模式：data 是完整 PresetTask
    await updateTask(data)
  } else {
    // 新增模式：data 是 { title, content }
    await addTask(data)
  }
  closeDialog()
}

async function handleDeleteTask(id: string) {
  await deleteTask(id)
}

/** 点击卡片主体 → session picker flow */
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

  // 仅一个活跃会话时跳过 picker，直接执行
  if (sessions.length === 1) {
    pendingSessionId.value = sessions[0].id
    showConfirmDialog.value = true
    return
  }

  // 多个活跃会话时始终显示 picker，让用户选择目标会话
  showSessionPicker.value = true
}

/** 从菜单执行 → 同样走 session picker */
function handleTaskExecute(task: PresetTask) {
  handleTaskTap(task)
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
