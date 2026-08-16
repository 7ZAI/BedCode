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

    <!-- 编辑区域 + 任务卡片（整体滚动，结构参照 auto-task Tab1：创建表单在上、卡片列表在下） -->
    <div ref="contentScrollRef" class="flex-1 overflow-y-auto overflow-x-hidden px-4 pb-8">
      <!-- 编辑区域：原编辑弹窗内容常驻页面顶部 -->
      <div class="group-card p-4 space-y-3">
        <div class="flex items-center justify-between">
          <label class="group-row-sub">{{ t(editingTask ? 'mobile.toolbox.editTask' : 'mobile.toolbox.taskContent') }}</label>
          <button
            v-if="!hasAiTemplate"
            class="flex items-center gap-1 px-2 py-0.5 rounded-md text-xs font-medium transition-transform active:scale-95"
            style="background: var(--mobile-accent-secondary); border: 1px solid var(--mobile-border-active); color: var(--mobile-accent)"
            @click="insertAiTemplate"
          >
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
            {{ t('mobile.toolbox.insertAiTemplate') }}
          </button>
        </div>

        <!-- 工程目录选择（有可用目录时显示） -->
        <div v-if="effectiveProjectDir" class="flex items-center gap-2">
          <div class="relative flex-1 min-w-0">
            <button
              class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs w-full transition-colors active:opacity-80"
              style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-border-hover); color: var(--mobile-text-secondary)"
              @click="showDirDropdown = !showDirDropdown"
            >
              <svg
                class="w-3.5 h-3.5 flex-shrink-0"
                style="color: var(--mobile-accent)"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                />
              </svg>
              <span class="truncate">{{ selectedDirLabel }}</span>
              <svg
                class="w-3 h-3 flex-shrink-0 ml-auto transition-transform duration-200"
                :class="{ 'rotate-180': showDirDropdown }"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
              </svg>
            </button>
            <Transition name="dropdown">
              <div
                v-if="showDirDropdown"
                class="absolute top-full left-0 right-0 mt-1 max-h-[180px] overflow-y-auto rounded-lg shadow-[0_8px_24px_rgba(0,0,0,0.4)] z-30"
                style="background: var(--mobile-bg-tertiary); border: 1px solid var(--mobile-border)"
                @click.stop
              >
                <button
                  v-for="dir in projectDirs"
                  :key="dir"
                  class="dropdown-item w-full text-left px-3 py-2 text-xs transition-colors flex items-center gap-2"
                  :class="dir === selectedDir ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-secondary)]'"
                  @click="selectedDir = dir; showDirDropdown = false"
                >
                  <svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                    />
                  </svg>
                  <span class="truncate">{{ dir }}</span>
                  <svg
                    v-if="dir === selectedDir"
                    class="w-3 h-3 flex-shrink-0 ml-auto"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </button>
              </div>
            </Transition>
          </div>
          <button
            class="flex-shrink-0 flex items-center justify-center p-1.5 rounded-lg transition-transform active:scale-[0.98]"
            style="
              background: var(--mobile-accent-secondary);
              border: 1px solid var(--mobile-border-active);
              color: var(--mobile-accent);
              min-width: 2.25rem;
              min-height: 2.25rem;
            "
            :disabled="!fileExplorerSessionId"
            @click="showFileExplorer = true"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
              />
            </svg>
          </button>
        </div>

        <textarea
          ref="contentTextarea"
          v-model="draftContent"
          rows="4"
          class="content-input w-full rounded-lg px-3 py-2.5 resize-none overflow-y-auto transition-colors duration-200"
          :placeholder="t('mobile.toolbox.taskContentPlaceholder')"
          @input="autosizeTextarea"
          @keydown="submitOnEnter(handleAdd)"
        ></textarea>

        <!-- 可重复/不可重复属性（创建时设置；编辑时可改，改属性不重置执行状态） -->
        <RepeatableToggle v-model="draftRepeatable" />

        <button
          class="w-full h-11 rounded-xl text-sm font-medium transition-transform active:scale-[0.98] flex items-center justify-center gap-2"
          :class="{ 'opacity-50': !draftContent.trim() }"
          style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)"
          :disabled="!draftContent.trim()"
          @click="handleAdd"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          {{ editingTask ? t('common.button.save') : t('mobile.toolbox.add') }}
        </button>
      </div>

      <!-- 任务卡片列表 -->
      <div v-if="tasks.length > 0" class="group-card mt-4">
        <PresetTaskCard
          v-for="task in tasks"
          :key="task.id"
          :task="task"
          @tap="handleTaskTap(task)"
          @execute="handleTaskTap(task)"
          @edit="openEditTask($event)"
          @delete="handleDeleteTask($event)"
        />
      </div>

      <!-- Empty state -->
      <div v-else class="group-card mt-4 p-6 text-center">
        <svg class="w-10 h-10 mx-auto mb-3" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
        </svg>
        <p class="group-row-sub mb-1">{{ t('mobile.toolbox.noTasks') }}</p>
        <p class="text-xs" style="color: var(--mobile-text-disabled)">{{ t('mobile.toolbox.presetEntryEmpty') }}</p>
      </div>
    </div>

    <!-- Confirm Delete Dialog -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div v-if="showDeleteConfirm" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0" style="background: var(--mobile-overlay-heavy)" @click="showDeleteConfirm = false"></div>
          <div class="relative w-full max-w-[clamp(280px,384px,440px)] rounded-2xl p-6 shadow-xl modal-panel" style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border)">
            <h3 class="page-title text-lg mb-2">{{ t('mobile.toolbox.confirmDeleteTask') }}</h3>
            <p class="text-sm rounded-lg p-3 mb-4 line-clamp-3" style="color: var(--mobile-row-title); background: var(--mobile-bg-primary)">{{ pendingDeleteTask?.content }}</p>

            <div class="flex gap-3">
              <button
                class="flex-1 h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
                style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
                @click="showDeleteConfirm = false"
              >
                {{ t('common.button.cancel') }}
              </button>
              <button
                class="flex-1 h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
                style="background: var(--mobile-error-muted); color: var(--mobile-error); border: 1px solid color-mix(in srgb, var(--mobile-error) 40%, transparent)"
                @click="confirmDeleteTask"
              >
                {{ t('common.button.delete') }}
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

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
              class="w-full mt-4 h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
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
                class="flex-1 h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
                style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
                @click="showConfirmDialog = false"
              >
                {{ t('common.button.cancel') }}
              </button>
              <button
                class="flex-1 h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
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

    <!-- File Explorer Dialog -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div
          v-if="showFileExplorer && fileExplorerSessionId"
          class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui"
        >
          <div class="absolute inset-0" style="background: var(--mobile-overlay-heavy)" @click="showFileExplorer = false"></div>
          <div
            class="relative w-full h-full rounded-2xl shadow-xl overflow-hidden flex flex-col modal-panel"
            style="background: var(--mobile-bg-card); border: 1px solid var(--mobile-border)"
          >
            <FileExplorer
              :session-id="fileExplorerSessionId"
              mode="emit"
              :title="selectedDirLabel"
              @close="showFileExplorer = false"
              @navigate-settings="handleNavigateSettings"
            />
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
 * 页面顶部为常驻编辑区域（原 TaskEditDialog 内容：任务内容 + AI 模板 + 工程目录浏览 + 添加按钮），
 * 点击添加生成预设任务卡片条，下方为可滚动的任务卡片列表（执行/编辑/删除）。
 * 结构与 auto-task 插件 Tab1「创建任务 + 预设任务列表」一致。
 */

import { ref, computed, onMounted, nextTick, defineAsyncComponent } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { useToast } from '@/composables/useToast'
import PresetTaskCard from '@/components/PresetTaskCard.vue'
import RepeatableToggle from '@/components/RepeatableToggle.vue'
import type { PresetTask } from '@/composables/model'

// 懒加载：FileExplorer 依赖 shiki 高亮引擎，避免首次进入本页时加载整个 shiki
const FileExplorer = defineAsyncComponent(() => import('@/components/FileExplorer.vue'))

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()
const { t } = useI18n()
const { tasks, load, addTask, updateTask, deleteTask, executeTask, reconcileWithQueue } = usePresetTasks()

const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')
const activeSessions = computed(() => connection.activeSessions.value || [])
const sessionConfigs = computed(() => connection.sessionConfigs.value || [])

// ==================== 工程目录选择 ====================

/** 从会话配置中提取去重的工程目录列表 */
const projectDirs = computed(() => {
  const dirs = sessionConfigs.value
    .map((c: any) => c.working_dir)
    .filter((d: string | undefined): d is string => !!d)
  return [...new Set(dirs)]
})

/** 是否有任何可用目录 */
const effectiveProjectDir = computed(() => projectDirs.value.length > 0)

const showDirDropdown = ref(false)
const selectedDir = ref<string | null>(null)
const showFileExplorer = ref(false)

/** 目录短标签：仅显示最后一段路径 */
function dirLabel(dir: string | null): string {
  if (!dir) return t('mobile.toolbox.selectProject')
  const parts = dir.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || dir
}

const selectedDirLabel = computed(() => dirLabel(selectedDir.value))

/** 根据目录找到对应的活跃会话 ID（用于 FileExplorer） */
const fileExplorerSessionId = computed(() => {
  const dir = selectedDir.value
  if (!dir) {
    return connection.activeSessionId.value || ''
  }
  const matchedConfig = sessionConfigs.value.find((c: any) => c.working_dir === dir)
  if (!matchedConfig) return connection.activeSessionId.value || ''
  const session = activeSessions.value.find(
    (s: any) => s.config_id === matchedConfig.id || s.configId === matchedConfig.id
  )
  return session?.id || matchedConfig.id || connection.activeSessionId.value || ''
})

// ==================== 顶部编辑区域 ====================

/** AI 编程提示词标准 4 要素模板（与原编辑弹窗一致） */
const AI_TEMPLATE = '目标：\n上下文：\n约束：\n完成条件：'

const contentTextarea = ref<HTMLTextAreaElement | null>(null)
const contentScrollRef = ref<HTMLDivElement | null>(null)
const draftContent = ref('')
/** 可重复/不可重复属性（默认可重复：与旧数据行为一致） */
const draftRepeatable = ref(true)
/** 非空时为编辑模式：内容来自对应任务，点击按钮保存修改 */
const editingTask = ref<PresetTask | null>(null)

/** 检测内容中是否已包含模板要素 */
const hasAiTemplate = computed(() => {
  const c = draftContent.value
  return c.includes('目标：') && c.includes('上下文：') && c.includes('约束：') && c.includes('完成条件：')
})

/** 插入 AI 提示词模板（仅当内容中尚未包含时） */
function insertAiTemplate() {
  if (hasAiTemplate.value) return
  const current = draftContent.value.trim()
  draftContent.value = current ? `${current}\n${AI_TEMPLATE}` : AI_TEMPLATE
}

/** 自动增高 textarea：先置 auto 再取 scrollHeight，钳制到上限后由 overflow-y 滚动 */
function autosizeTextarea() {
  const el = contentTextarea.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, 280)}px`
}

/** 回车提交（IME 组词中的回车不触发），Shift+Enter 换行 */
function submitOnEnter(handler: () => void) {
  return (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      handler()
    }
  }
}

/** 添加或保存修改：生成/更新任务卡片条，并保持输入框焦点方便连续添加 */
async function handleAdd() {
  const content = draftContent.value.trim()
  if (!content) return

  if (editingTask.value) {
    await updateTask({ ...editingTask.value, content, repeatable: draftRepeatable.value })
  } else {
    await addTask({ content, repeatable: draftRepeatable.value })
  }

  draftContent.value = ''
  editingTask.value = null
  draftRepeatable.value = true
  nextTick(() => {
    autosizeTextarea()
    contentTextarea.value?.focus()
  })
}

/** 卡片编辑：把任务内容载入顶部编辑区域，滚动到页面顶部并聚焦 */
function openEditTask(task: PresetTask) {
  editingTask.value = task
  draftContent.value = task.content
  draftRepeatable.value = task.repeatable
  contentScrollRef.value?.scrollTo({ top: 0, behavior: 'smooth' })
  nextTick(() => {
    autosizeTextarea()
    contentTextarea.value?.focus()
  })
}

async function handleDeleteTask(id: string) {
  pendingDeleteTask.value = tasks.value.find(t => t.id === id) || null
  showDeleteConfirm.value = true
}

async function confirmDeleteTask() {
  const id = pendingDeleteTask.value?.id
  if (!id) return
  showDeleteConfirm.value = false
  pendingDeleteTask.value = null
  await deleteTask(id)
  // 删除的正是编辑中的任务时退出编辑模式
  if (editingTask.value?.id === id) {
    editingTask.value = null
    draftContent.value = ''
  }
}

/** 文件浏览未连接（base URL 缺失）→ 引导去连接设置 */
function handleNavigateSettings() {
  showFileExplorer.value = false
  router.push({ name: 'mobile-settings-connection' })
}

// ==================== 执行流程 ====================

const showSessionPicker = ref(false)
const showConfirmDialog = ref(false)
const pendingTask = ref<PresetTask | null>(null)
const pendingSessionId = ref('')

// 删除确认弹窗（防误触）
const showDeleteConfirm = ref(false)
const pendingDeleteTask = ref<PresetTask | null>(null)

onMounted(async () => {
  await load()
  // 对账（spec：面板打开 + 应用启动后首次进入）：执行中且队列项已不在
  // 当前会话 pending 队列的预设落中断（广播不可靠时的兑底）
  const sid = connection.activeSessionId.value
  if (sid) {
    await reconcileWithQueue(sid)
  }
})

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

<style scoped>
/* 任务内容输入框：token 绑定 + 聚焦态（配合 autosizeTextarea 的 JS 高度自适应） */
.content-input {
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border-hover);
  color: var(--mobile-text-primary);
  font-size: var(--font-size-base);
  line-height: 1.5;
  max-height: 280px;
  outline: none;
}

.content-input::placeholder {
  color: var(--mobile-text-disabled);
}

.content-input:focus {
  border-color: color-mix(in srgb, var(--mobile-accent) 50%, transparent);
}

/* 下拉菜单过渡 */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: all 0.2s ease;
}

.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

.dropdown-item:hover {
  background: var(--mobile-bg-elevated);
}
</style>
