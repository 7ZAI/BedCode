<template>
  <div class="att-tab-body">
    <div ref="contentScrollRef" class="att-scroll">
      <div class="att-scroll-inner">
        <!-- 编辑区域（常驻页面顶部，还原旧宿主 PresetTasksView 形态）：任务内容 + AI 模板 +
             工程目录选择 + 可重复属性 + 添加/保存按钮，添加/编辑都不走弹窗 -->
        <div class="group-card p-4 space-y-3">
          <div class="flex items-center justify-between">
            <label class="group-row-sub">{{ t(editingTask ? 'toolbox.editTask' : 'toolbox.taskContent') }}</label>
            <button
              v-if="!hasAiTemplate"
              class="flex items-center gap-1 px-2 py-0.5 rounded-md text-xs font-medium transition-transform active:scale-95"
              style="background: var(--mobile-accent-secondary); border: 1px solid var(--mobile-border-active); color: var(--mobile-accent)"
              @click="insertAiTemplate"
            >
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              {{ t('toolbox.insertAiTemplate') }}
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
            :placeholder="t('toolbox.taskContentPlaceholder')"
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
            {{ editingTask ? t('common.button.save') : t('toolbox.add') }}
          </button>
        </div>

        <!-- 预设任务列表（容器是 group-card，卡片自持 group-row 语汇与行间分隔线） -->
        <div v-if="tasks.length > 0" class="group-card mt-3">
          <PresetTaskCard
            v-for="task in tasks"
            :key="task.id"
            :task="task"
            @tap="startExecuteFlow(task)"
            @execute="startExecuteFlow(task)"
            @edit="openEditTask"
            @delete="askDelete"
          />
        </div>

        <!-- 空态：说明「预设任务」是干什么的，降低首屏困惑 -->
        <div v-else class="att-state">
          <svg class="att-state-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M9 4h6a1 1 0 0 1 1 1v1h1a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h1V5a1 1 0 0 1 1-1z"
            />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="m9 13 2 2 4-4" />
          </svg>
          <p>{{ t('preset.noTasks') }}</p>
          <p class="text-[var(--mobile-text-disabled)] text-[var(--font-size-sm)]">
            {{ t('preset.emptyHint') }}
          </p>
        </div>
      </div>
    </div>

    <!-- 文件浏览：宿主通用组件（代码浏览域），经 bedcodeHostComponents 注入 -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div
          v-if="showFileExplorer && fileExplorerSessionId"
          class="fixed inset-0 z-[100] flex items-center justify-center p-[10%] mobile-ui"
        >
          <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showFileExplorer = false"></div>
          <div class="relative w-full h-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl shadow-xl overflow-hidden flex flex-col modal-panel">
            <component
              :is="hostComponents.FileExplorer"
              v-if="hostComponents.FileExplorer"
              :session-id="fileExplorerSessionId"
              mode="emit"
              :title="effectiveDirLabel"
              @close="showFileExplorer = false"
              @navigate-settings="handleNavigateSettings"
            />
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 删除二次确认（防误触） -->
    <Modal v-model="showDeleteConfirm" :title="t('preset.delete')" size="sm">
      <p class="rounded-lg p-3 line-clamp-3" style="background: var(--mobile-bg-primary); color: var(--mobile-row-title)">
        {{ pendingDeleteTask?.content }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirm = false">{{ t('cancel') }}</Button>
          <Button variant="danger" @click="confirmDelete">{{ t('delete') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- 会话选择：多活跃会话时先让用户选目标，再进确认（单会话直接跳确认） -->
    <Modal v-model="showSessionPicker" :title="t('preset.selectSession')" size="sm">
      <p v-if="activeSessions.length === 0" class="group-row-sub text-center py-2">
        {{ t('preset.noActiveSessions') }}
      </p>
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
      <template #footer>
        <div class="flex justify-end">
          <Button variant="ghost" @click="showSessionPicker = false">{{ t('cancel') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- 执行确认：把将要发出的指令原文摆出来，避免误发 -->
    <Modal v-model="showExecuteConfirm" :title="t('preset.confirmExecute')" size="sm">
      <p class="text-[var(--font-size-base)] mb-1" style="color: var(--mobile-text-muted)">
        {{ t('preset.willSendToTerminal') }}
      </p>
      <p class="rounded-lg p-3 line-clamp-3" style="background: var(--mobile-bg-primary); color: var(--mobile-row-title)">
        {{ pendingTask?.content }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showExecuteConfirm = false">{{ t('cancel') }}</Button>
          <Button @click="doExecute">{{ t('preset.execute') }}</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * PresetTasksTab — 预设任务页签
 *
 * 还原旧宿主 `src/views/PresetTasksView.vue`（票 2026-10-09 阶段 B 随旧宿主 UI
 * 退役删除，`b84767916` 前最后一个版本）。旧页是二级路由页，自带页头与返回键；本页是
 * 任务域的首个页签，页头由 app 域统一出，故去掉旧页头与返回，标题语义由任务页承担。
 *
 * 与弹窗版实现（`TaskEditDialog`）的关键差异：**新增 / 编辑不弹窗**——编辑表单
 * （任务内容 + AI 模板 + 工程目录 + 可重复属性 + 添加/保存按钮）常驻页面顶部，
 * 语义与交互对齐旧宿主 PresetTasksView；文件浏览 / 删除确认 / 会话选择 / 执行确认
 * 仍走弹层（与旧页一致，弹层是「选择或确认」而非「录入」）。
 *
 * 真源经 SDK `getPresetTasks()`（宿主 usePresetTasks 共享模块）读写，本页不持有状态；
 * 与终端任务选择器、任务面板同一份预设任务真源。
 * 未连接时的引导由「toast + 跳转连接页」改为 toast + 向 app 域发 `open-connection`
 * （与会话页的 `@open-connection` 同一通道，页签切换归 app 域）。
 */
import { ref, computed, onMounted, nextTick, inject } from 'vue'
import { getMobileApi, getPresetTasks } from '@binblink/bedcode-plugin-sdk-mobile'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import type { PresetTask } from '../../terminal/model'
import PresetTaskCard from './PresetTaskCard.vue'
import RepeatableToggle from '../../terminal/components/RepeatableToggle.vue'
import Modal from '../../host/components/ui/Modal.vue'
import Button from '../../host/components/ui/Button.vue'

const props = defineProps<{ context: PluginContext }>()

const emit = defineEmits<{ 'open-connection': [] }>()

const t = (key: string): string => props.context.i18n.t(key)
/** 轻提示（宿主 toast；宿主 dialogs 只有一个 showToast(message, type) 函数，非对象） */
const toast = (message: string, type: 'info' | 'success' | 'warning' | 'error' = 'info') =>
  props.context.dialogs.showToast(message, type)

/** 宿主壳 provide 的通用组件（FileExplorer 属代码浏览域，不随终端域迁移） */
const hostComponents = inject<{ FileExplorer?: any }>('bedcodeHostComponents', {})

const connection = getMobileApi()
const { tasks, load, addTask, updateTask, deleteTask, executeTask, reconcileWithQueue } =
  getPresetTasks().usePresetTasks()

// ==================== 宿主连接事实 ====================

const isConnected = computed(() => connection.isConnected.value === true)
const activeSessionId = computed(() => connection.activeSessionId.value || '')
const activeSessions = computed<any[]>(() => connection.activeSessions.value || [])
const sessionConfigs = computed<any[]>(() => connection.sessionConfigs.value || [])

/** 可选工程目录（去重后的会话配置工作目录，供编辑表单的目录选择器使用） */
const projectDirs = computed(() => {
  const dirs = sessionConfigs.value
    .map((c: any) => c.working_dir)
    .filter((d: unknown): d is string => !!d)
  return [...new Set(dirs)]
})

// ==================== 顶部编辑区域（常驻，非弹窗） ====================

/** AI 编程提示词标准 4 要素模板（与原 TaskEditDialog 一致） */
const AI_TEMPLATE = '目标：\n上下文：\n约束：\n完成条件：'

const contentScrollRef = ref<HTMLDivElement | null>(null)
const contentTextarea = ref<HTMLTextAreaElement | null>(null)
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
function insertAiTemplate(): void {
  if (hasAiTemplate.value) return
  const current = draftContent.value.trim()
  draftContent.value = current ? `${current}\n${AI_TEMPLATE}` : AI_TEMPLATE
  autosizeTextarea()
}

/** 自动增高 textarea：先置 auto 再取 scrollHeight，钳制到上限后由 overflow-y 滚动 */
function autosizeTextarea(): void {
  const el = contentTextarea.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, 280)}px`
}

/** 回车提交（IME 组词中的回车不触发），Shift+Enter 换行 */
function submitOnEnter(handler: () => void) {
  return (e: KeyboardEvent): void => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      handler()
    }
  }
}

/** 添加或保存修改：生成/更新任务卡片条，并保持输入框焦点方便连续添加 */
async function handleAdd(): Promise<void> {
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
function openEditTask(task: PresetTask): void {
  editingTask.value = task
  draftContent.value = task.content
  draftRepeatable.value = task.repeatable
  contentScrollRef.value?.scrollTo?.({ top: 0, behavior: 'smooth' })
  nextTick(() => {
    autosizeTextarea()
    contentTextarea.value?.focus()
  })
}

// ==================== 工程目录选择 ====================

const showDirDropdown = ref(false)
const selectedDir = ref<string | null>(null)
const showFileExplorer = ref(false)

/** 是否有任何可用目录（决定是否显示目录区域） */
const effectiveProjectDir = computed(() => projectDirs.value.length > 0)

/** 目录短标签：仅显示最后一段路径 */
function dirLabel(dir: string | null): string {
  if (!dir) return t('toolbox.selectProject')
  const parts = dir.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || dir
}

const selectedDirLabel = computed(() => dirLabel(selectedDir.value))

/** 用于 FileExplorer 标题的目录标签 */
const effectiveDirLabel = computed(() => dirLabel(selectedDir.value))

/** 根据目录找到对应的活跃会话 ID（用于 FileExplorer） */
const fileExplorerSessionId = computed(() => {
  const dir = selectedDir.value
  if (!dir) {
    return activeSessionId.value || ''
  }
  const matchedConfig = sessionConfigs.value.find((c: any) => c.working_dir === dir)
  if (!matchedConfig) return activeSessionId.value || ''
  const session = activeSessions.value.find(
    (s: any) => s.config_id === matchedConfig.id || s.configId === matchedConfig.id
  )
  // 优先使用活跃会话 id；无可运行会话时回退到 config_id，桌面端文件 API 支持直接用 config_id 浏览
  return session?.id || matchedConfig.id || ''
})

/** 文件浏览未连接（base URL 缺失）→ 关闭浏览弹层并引导回连接页 */
function handleNavigateSettings(): void {
  showFileExplorer.value = false
  emit('open-connection')
}

// ==================== 删除 ====================

const showDeleteConfirm = ref(false)
const pendingDeleteTask = ref<PresetTask | null>(null)

function askDelete(id: string): void {
  pendingDeleteTask.value = tasks.value.find((task: PresetTask) => task.id === id) ?? null
  showDeleteConfirm.value = true
}

async function confirmDelete(): Promise<void> {
  const id = pendingDeleteTask.value?.id
  showDeleteConfirm.value = false
  pendingDeleteTask.value = null
  if (!id) return
  await deleteTask(id)
  // 删掉的正是正在编辑的任务时一并退出编辑态，避免下次保存带出已不存在的任务
  if (editingTask.value?.id === id) {
    editingTask.value = null
    draftContent.value = ''
  }
}

// ==================== 执行流程 ====================

const showSessionPicker = ref(false)
const showExecuteConfirm = ref(false)
const pendingTask = ref<PresetTask | null>(null)
const pendingSessionId = ref('')

/**
 * 执行入口：按活跃会话数分三档
 * 未连接 → toast 引导去连接页；0 会话 → toast 提示；1 会话 → 直接进确认；
 * 多会话 → 先选会话再确认（不猜用户想发到哪个终端）
 */
function startExecuteFlow(task: PresetTask): void {
  pendingTask.value = task

  if (!isConnected.value) {
    toast(t('preset.connectFirst'), 'warning')
    emit('open-connection')
    return
  }
  if (activeSessions.value.length === 0) {
    toast(t('preset.noActiveSessions'), 'warning')
    return
  }
  if (activeSessions.value.length === 1) {
    pendingSessionId.value = activeSessions.value[0].id
    showExecuteConfirm.value = true
    return
  }
  showSessionPicker.value = true
}

function confirmExecute(sessionId: string): void {
  showSessionPicker.value = false
  pendingSessionId.value = sessionId
  showExecuteConfirm.value = true
}

async function doExecute(): Promise<void> {
  const task = pendingTask.value
  const sessionId = pendingSessionId.value
  showExecuteConfirm.value = false
  if (!task || !sessionId) return

  try {
    await executeTask(task, sessionId)
    toast(t('preset.sentToTerminal'), 'success')
  } catch (e) {
    props.context.logger.error(
      `[task] preset task execute failed: ${e instanceof Error ? e.message : String(e)}`,
    )
    toast(t('preset.sendFailed'), 'error')
  }
  pendingTask.value = null
  pendingSessionId.value = ''
}

onMounted(async () => {
  await load()
  // 对账：进入本页时，若有「执行中」预设的队列项已不在该会话 pending 队列中，
  // 落「已中断」（WS 广播不可靠时的兜底，语义与旧 PresetTasksView 一致）
  const sid = activeSessionId.value
  if (sid) await reconcileWithQueue(sid)
})
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