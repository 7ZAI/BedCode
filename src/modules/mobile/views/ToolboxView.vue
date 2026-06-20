<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">工具箱</h1>
    </header>

    <!-- Toolbox Sections -->
    <div class="flex-1 overflow-auto p-4 space-y-5">

      <!-- Section: 预设任务 -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium tracking-wider uppercase">预设任务</h3>
          <button
            class="text-xs text-[var(--mobile-accent)] hover:text-cyan-300 transition-colors flex items-center gap-1"
            @click="openAddDialog"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
            添加
          </button>
        </div>

        <!-- Empty state -->
        <div
          v-if="tasks.length === 0"
          class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 text-center"
        >
          <p class="text-[var(--mobile-text-disabled)] text-sm">暂无预设任务</p>
          <button
            class="mt-2 text-xs text-[var(--mobile-accent)] hover:text-cyan-300 transition-colors"
            @click="openAddDialog"
          >
            + 添加任务
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

      <!-- Section: 插件（预留） -->
      <section>
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">插件</h3>
        <div class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 text-center">
          <p class="text-[var(--mobile-text-disabled)] text-sm">即将推出</p>
        </div>
      </section>

    </div>

    <!-- Add/Edit Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/80" @click="closeDialog"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border-hover)] rounded-2xl p-6">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4">
              {{ editingTask ? '编辑预设任务' : '添加预设任务' }}
            </h3>

            <div class="space-y-4">
              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">任务标题</label>
                <input
                  v-model="dialogForm.title"
                  type="text"
                  placeholder="任务标题"
                  class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-cyan-500/50 transition-colors"
                />
              </div>

              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">任务内容</label>
                <textarea
                  v-model="dialogForm.content"
                  placeholder="发送到终端的指令内容"
                  rows="3"
                  class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-cyan-500/50 transition-colors resize-none"
                ></textarea>
              </div>

              <!-- 任务类型 radio toggle（编辑时禁用） -->
              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-2 block">任务类型</label>
                <div class="flex gap-3">
                  <button
                    class="flex-1 py-2 rounded-lg text-sm font-medium border transition-colors"
                    :class="dialogForm.type === 'once'
                      ? 'bg-amber-500/15 border-amber-500/30 text-amber-400'
                      : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)] text-[var(--mobile-text-muted)]'"
                    :disabled="!!editingTask"
                    @click="dialogForm.type = 'once'"
                  >
                    一次性
                  </button>
                  <button
                    class="flex-1 py-2 rounded-lg text-sm font-medium border transition-colors"
                    :class="dialogForm.type === 'template'
                      ? 'bg-cyan-500/15 border-cyan-500/30 text-cyan-400'
                      : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)] text-[var(--mobile-text-muted)]'"
                    :disabled="!!editingTask"
                    @click="dialogForm.type = 'template'"
                  >
                    模板
                  </button>
                </div>
                <p v-if="editingTask" class="text-[10px] text-[var(--mobile-text-disabled)] mt-1">创建后类型不可更改</p>
              </div>
            </div>

            <div class="flex gap-3 mt-6">
              <button
                class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-cyan-500/40 transition-colors"
                @click="closeDialog"
              >
                取消
              </button>
              <button
                class="flex-1 bg-cyan-500/20 border border-cyan-500/30 text-[var(--mobile-accent)] py-2.5 rounded-xl font-medium hover:bg-cyan-500/30 transition-colors"
                :class="{ 'opacity-50': !dialogForm.title || !dialogForm.content }"
                :disabled="!dialogForm.title || !dialogForm.content"
                @click="saveTask"
              >
                保存
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Session Picker Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/80" @click="showSessionPicker = false"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border-hover)] rounded-2xl p-6">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4">选择会话</h3>

            <div v-if="activeSessions.length === 0" class="text-center py-4">
              <p class="text-[var(--mobile-text-disabled)] text-sm">暂无活跃会话</p>
            </div>

            <div v-else class="space-y-2 max-h-60 overflow-y-auto">
              <button
                v-for="session in activeSessions"
                :key="session.id"
                class="w-full text-left px-4 py-3 rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-primary)] hover:border-cyan-500/30 transition-colors"
                @click="confirmExecute(session.id)"
              >
                <p class="text-sm font-medium text-[var(--mobile-text-primary)]">{{ session.name }}</p>
                <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5">{{ session.id.slice(0, 8) }}</p>
              </button>
            </div>

            <button
              class="w-full mt-4 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-cyan-500/40 transition-colors"
              @click="showSessionPicker = false"
            >
              取消
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Confirm Execute Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/80" @click="showConfirmDialog = false"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border-hover)] rounded-2xl p-6">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-2">确认执行</h3>
            <p class="text-sm text-[var(--mobile-text-muted)] mb-1">将发送到终端：</p>
            <p class="text-sm text-[var(--mobile-text-primary)] bg-[var(--mobile-bg-primary)] rounded-lg p-3 mb-4 line-clamp-3">{{ pendingTask?.content }}</p>

            <div class="flex gap-3">
              <button
                class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-cyan-500/40 transition-colors"
                @click="showConfirmDialog = false"
              >
                取消
              </button>
              <button
                class="flex-1 bg-cyan-500/20 border border-cyan-500/30 text-[var(--mobile-accent)] py-2.5 rounded-xl font-medium hover:bg-cyan-500/30 transition-colors"
                @click="doExecute"
              >
                执行
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
 * 预设任务管理 + 插件（预留）
 */

import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { usePresetTasks } from '@/modules/mobile/composables/usePresetTasks'
import { useToast } from '@/modules/shared/composables/useToast'
import PresetTaskCard from '@/modules/mobile/components/PresetTaskCard.vue'
import type { PresetTask, PresetTaskType } from '@/modules/mobile/composables/model'

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()
const { tasks, load, addTask, updateTask, deleteTask, executeTask } = usePresetTasks()

const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')
const activeSessionId = computed(() => connection.activeSessionId.value || '')
const activeSessions = computed(() => connection.activeSessions.value || [])

// ==================== 预设任务 ====================

const showDialog = ref(false)
const editingTask = ref<PresetTask | null>(null)
const dialogForm = ref<{ title: string; content: string; type: PresetTaskType }>({
  title: '',
  content: '',
  type: 'once',
})

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
  dialogForm.value = { title: '', content: '', type: 'once' }
  showDialog.value = true
}

function openEditDialog(task: PresetTask) {
  editingTask.value = task
  dialogForm.value = { title: task.title, content: task.content, type: task.type }
  showDialog.value = true
}

function closeDialog() {
  showDialog.value = false
  editingTask.value = null
}

async function saveTask() {
  if (!dialogForm.value.title || !dialogForm.value.content) return

  if (editingTask.value) {
    await updateTask({
      ...editingTask.value,
      title: dialogForm.value.title,
      content: dialogForm.value.content,
    })
  } else {
    await addTask({
      title: dialogForm.value.title,
      content: dialogForm.value.content,
      type: dialogForm.value.type,
    })
  }

  closeDialog()
}

async function handleDeleteTask(id: string) {
  await deleteTask(id)
}

/** 点击卡片主体 → session picker flow */
function handleTaskTap(task: PresetTask) {
  pendingTask.value = task
  const sessionId = activeSessionId.value

  if (!isConnected.value || !sessionId) {
    toast.warning('请先连接设备')
    router.push('/mobile/devices')
    return
  }

  // 仅一个活跃会话时跳过 picker
  const sessions = activeSessions.value
  if (sessions.length <= 1) {
    pendingSessionId.value = sessionId
    showConfirmDialog.value = true
    return
  }

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
    toast.success('已发送到终端')
  } catch {
    toast.error('发送失败')
  }

  pendingTask.value = null
  pendingSessionId.value = ''
}

</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
