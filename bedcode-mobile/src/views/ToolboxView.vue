<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center justify-between gap-2">
      <!-- 左侧：项目目录选择器 -->
      <div class="flex items-center gap-2 min-w-0 flex-1">
        <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide flex-shrink-0">{{ t('mobile.toolbox.title') }}</h1>
        <div v-if="isConnected && projectDirs.length > 0" class="relative min-w-0">
          <button
            class="flex items-center gap-1 px-2 py-1 rounded-lg bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] text-[var(--mobile-text-secondary)] text-xs max-w-[160px] hover:border-[var(--mobile-border-active)] active:opacity-80 transition-colors"
            @click="showDirDropdown = !showDirDropdown"
          >
            <svg class="w-3.5 h-3.5 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            <span class="truncate">{{ selectedDirLabel }}</span>
            <svg class="w-3 h-3 flex-shrink-0 transition-transform duration-200" :class="{ 'rotate-180': showDirDropdown }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
            </svg>
          </button>
          <Transition name="dropdown">
            <div v-if="showDirDropdown" class="absolute top-full left-0 mt-1 min-w-[200px] max-w-[280px] max-h-[240px] overflow-y-auto bg-[var(--mobile-bg-tertiary)] border border-[var(--mobile-border)] rounded-lg shadow-[0_8px_24px_rgba(0,0,0,0.4)] z-30" @click.stop>
              <button
                v-for="dir in projectDirs"
                :key="dir"
                class="w-full text-left px-3 py-2.5 text-sm hover:bg-[var(--mobile-bg-elevated)] active:bg-[var(--mobile-bg-primary)] transition-colors flex items-center gap-2"
                :class="dir === selectedDir ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-secondary)]'"
                @click="selectDir(dir)"
              >
                <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                <span class="truncate">{{ dir }}</span>
                <svg v-if="dir === selectedDir" class="w-3.5 h-3.5 flex-shrink-0 ml-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                </svg>
              </button>
            </div>
          </Transition>
        </div>
      </div>
      <!-- 右侧：查看文件按钮 -->
      <button
        class="p-2 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] active:bg-[var(--mobile-bg-secondary)] rounded-lg transition-colors flex-shrink-0"
        :class="{ 'text-[var(--mobile-accent)]': showFileSidebar }"
        :disabled="!isConnected || !sidebarSessionId"
        :title="!isConnected ? t('mobile.toolbox.connectFirst') : t('mobile.toolbox.browseFiles')"
        @click="toggleFileSidebar"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
      </button>
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

      <!-- File Sidebar - 覆盖层模式，和终端页一致 -->
      <FileSidebar
        v-if="sidebarSessionId"
        class="sidebar-overlay"
        :class="{ 'sidebar-hidden': !showFileSidebar }"
        :session-id="sidebarSessionId"
        mode="standalone"
        resize-side="left"
      />

      <!-- 点击侧边栏外部关闭 -->
      <div v-if="showFileSidebar" class="sidebar-backdrop" @click="showFileSidebar = false"></div>
    </div>

    <!-- Add/Edit Dialog -->
    <Teleport to="body">
      <Transition name="modal">
        <div v-if="showDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="closeDialog"></div>
          <div class="relative w-full max-w-lg bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl max-h-[85vh] flex flex-col">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4 flex-shrink-0">
              {{ editingTask ? t('mobile.toolbox.editTask') : t('mobile.toolbox.addTaskTitle') }}
            </h3>

            <div class="space-y-4 flex-1 overflow-y-auto min-h-0">
              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">{{ t('mobile.toolbox.taskTitle') }}</label>
                <input
                  v-model="dialogForm.title"
                  type="text"
                  :placeholder="t('mobile.toolbox.taskTitlePlaceholder')"
                  class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2.5 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-[var(--mobile-accent)]/50 transition-colors"
                />
              </div>

              <div class="flex-1 min-h-0 flex flex-col">
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">{{ t('mobile.toolbox.taskContent') }}</label>
                <textarea
                  v-model="dialogForm.content"
                  :placeholder="t('mobile.toolbox.taskContentPlaceholder')"
                  rows="8"
                  class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2.5 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-[var(--mobile-accent)]/50 transition-colors resize-none flex-1 min-h-[160px]"
                ></textarea>
              </div>

              <!-- 浏览工程目录 -->
              <div v-if="isConnected && projectDirs.length > 0">
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">{{ t('mobile.toolbox.browseProject') }}</label>
                <div class="flex gap-2">
                  <div class="relative flex-1 min-w-0">
                    <button
                      class="w-full flex items-center gap-1.5 px-3 py-2.5 rounded-lg bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] text-sm hover:border-[var(--mobile-border-active)] active:opacity-80 transition-colors"
                      @click="showDialogDirDropdown = !showDialogDirDropdown"
                    >
                      <svg class="w-3.5 h-3.5 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                      </svg>
                      <span class="truncate">{{ selectedDirLabel }}</span>
                      <svg class="w-3 h-3 flex-shrink-0 ml-auto transition-transform duration-200" :class="{ 'rotate-180': showDialogDirDropdown }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                      </svg>
                    </button>
                    <Transition name="dropdown">
                      <div v-if="showDialogDirDropdown" class="absolute top-full left-0 right-0 mt-1 max-h-[180px] overflow-y-auto bg-[var(--mobile-bg-tertiary)] border border-[var(--mobile-border)] rounded-lg shadow-[0_8px_24px_rgba(0,0,0,0.4)] z-30" @click.stop>
                        <button
                          v-for="dir in projectDirs"
                          :key="dir"
                          class="w-full text-left px-3 py-2.5 text-sm hover:bg-[var(--mobile-bg-elevated)] active:bg-[var(--mobile-bg-primary)] transition-colors flex items-center gap-2"
                          :class="dir === selectedDir ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-secondary)]'"
                          @click="selectDir(dir); showDialogDirDropdown = false"
                        >
                          <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                          </svg>
                          <span class="truncate">{{ dir }}</span>
                          <svg v-if="dir === selectedDir" class="w-3.5 h-3.5 flex-shrink-0 ml-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                          </svg>
                        </button>
                      </div>
                    </Transition>
                  </div>
                  <button
                    class="px-3 py-2.5 rounded-lg bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] text-sm font-medium hover:bg-[var(--mobile-accent)]/30 active:scale-[0.98] transition-all duration-150 flex-shrink-0"
                    :disabled="!sidebarSessionId"
                    @click="showFileSidebar = !showFileSidebar"
                  >
                    <svg class="w-4 h-4" :class="{ 'text-[var(--mobile-accent)]': showFileSidebar }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                    </svg>
                  </button>
                </div>
              </div>

              <!-- 任务类型 radio toggle（编辑时禁用） -->
              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-2 block">{{ t('mobile.toolbox.taskType') }}</label>
                <div class="flex gap-3">
                  <button
                    class="flex-1 py-2 rounded-lg text-sm font-medium border transition-colors"
                    :class="dialogForm.type === 'once'
                      ? 'bg-[var(--mobile-warning-muted)] border-[var(--mobile-warning)]/30 text-[var(--mobile-warning)]'
                      : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)] text-[var(--mobile-text-muted)]'"
                    :disabled="!!editingTask"
                    @click="dialogForm.type = 'once'"
                  >
                    {{ t('mobile.presetTask.once') }}
                  </button>
                  <button
                    class="flex-1 py-2 rounded-lg text-sm font-medium border transition-colors"
                    :class="dialogForm.type === 'template'
                      ? 'bg-[var(--mobile-accent-muted)] border-[var(--mobile-accent)]/30 text-[var(--mobile-accent)]'
                      : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)] text-[var(--mobile-text-muted)]'"
                    :disabled="!!editingTask"
                    @click="dialogForm.type = 'template'"
                  >
                    {{ t('mobile.presetTask.template') }}
                  </button>
                </div>
                <p v-if="editingTask" class="text-[10px] text-[var(--mobile-text-disabled)] mt-1">{{ t('mobile.toolbox.typeCannotChange') }}</p>
              </div>
            </div>

            <div class="flex gap-3 mt-6 flex-shrink-0">
              <button
                class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-[var(--mobile-accent)]/40 active:opacity-80 transition-colors"
                @click="closeDialog"
              >
                {{ t('common.button.cancel') }}
              </button>
              <button
                class="flex-1 bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] py-2.5 rounded-xl font-medium hover:bg-[var(--mobile-accent)]/30 active:scale-[0.98] transition-all duration-150"
                :class="{ 'opacity-50': !dialogForm.title || !dialogForm.content }"
                :disabled="!dialogForm.title || !dialogForm.content"
                @click="saveTask"
              >
                {{ t('common.button.save') }}
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Session Picker Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showSessionPicker = false"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl">
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
      <Transition name="fade">
        <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showConfirmDialog = false"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl">
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

import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { useToast } from '@/composables/useToast'
import PresetTaskCard from '@/components/PresetTaskCard.vue'
import FileSidebar from '@/components/FileSidebar.vue'
import type { PresetTask, PresetTaskType } from '@/composables/model'

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()
const { t } = useI18n()
const { tasks, load, addTask, updateTask, deleteTask, executeTask } = usePresetTasks()

const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')
const activeSessionId = computed(() => connection.activeSessionId.value || '')
const activeSessions = computed(() => connection.activeSessions.value || [])

// ==================== 项目目录选择 ====================

const showDirDropdown = ref(false)
const showDialogDirDropdown = ref(false)
const selectedDir = ref<string | null>(null)

/** 从会话配置中提取去重的工程目录列表 */
const projectDirs = computed(() => {
  const configs = connection.sessionConfigs.value || []
  const dirs = configs
    .map(c => c.working_dir)
    .filter((d): d is string => !!d)
  return [...new Set(dirs)]
})

/** 选中目录的短标签（仅显示最后一段路径） */
const selectedDirLabel = computed(() => {
  if (!selectedDir.value) return t('mobile.toolbox.selectProject')
  const parts = selectedDir.value.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || selectedDir.value
})

/** 根据选中目录找到对应的活跃会话 ID */
const sidebarSessionId = computed(() => {
  if (!selectedDir.value) {
    // 未选择目录时 fallback 到当前活跃会话
    return activeSessionId.value
  }
  const configs = connection.sessionConfigs.value || []
  const matchedConfig = configs.find(c => c.working_dir === selectedDir.value)
  if (!matchedConfig) return ''
  // 查找该配置的活跃会话
  const session = activeSessions.value.find(
    (s: any) => s.config_id === matchedConfig.id || s.configId === matchedConfig.id
  )
  return session?.id || ''
})

function selectDir(dir: string) {
  selectedDir.value = dir
  showDirDropdown.value = false
}

// 点击外部关闭下拉
function onClickOutside(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (!target.closest('.dir-dropdown-trigger') && !target.closest('[data-dir-dropdown]')) {
    showDirDropdown.value = false
    showDialogDirDropdown.value = false
  }
}

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

// ==================== 文件侧栏 ====================

const showFileSidebar = ref(false)

function toggleFileSidebar() {
  if (!isConnected.value || !sidebarSessionId.value) {
    toast.warning(t('mobile.toolbox.connectFirst'))
    return
  }
  showFileSidebar.value = !showFileSidebar.value
}

onMounted(async () => {
  await load()
  document.addEventListener('click', onClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('click', onClickOutside)
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

<style scoped>
/* Modal transition - scale + fade */
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from > :last-child,
.modal-leave-to > :last-child {
  transform: scale(0.95);
}

/* Fade transition */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* Dropdown transition */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: all 0.2s ease;
}

.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

/* File sidebar overlay - 和终端页一致的滑入/滑出动画 */
.sidebar-overlay {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 40%;
  min-width: 200px;
  max-width: 320px;
  z-index: 51;
  transform: translateX(0);
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

.sidebar-hidden {
  transform: translateX(100%);
}

.sidebar-backdrop {
  position: absolute;
  inset: 0;
  z-index: 50;
  background: var(--mobile-overlay);
}
</style>
