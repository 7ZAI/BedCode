<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div v-if="visible" class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui">
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="emit('close')"></div>
        <div class="relative w-full max-w-lg bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl max-h-[85vh] flex flex-col modal-panel">
          <div class="flex items-center justify-between mb-4 flex-shrink-0">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)]">
              {{ task ? t('mobile.toolbox.editTask') : t('mobile.toolbox.addTaskTitle') }}
            </h3>
            <!-- 浏览工程目录 -->
            <div v-if="isConnected && effectiveProjectDir" class="flex items-center gap-1.5">
              <!-- 锁定目录：只读标签，不可切换 -->
              <div
                v-if="lockedDir"
                class="flex items-center gap-1 px-2 py-1 rounded-lg bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] text-xs max-w-[140px]"
              >
                <svg class="w-3 h-3 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                <span class="truncate">{{ lockedDirLabel }}</span>
              </div>
              <!-- 可选目录：下拉菜单 -->
              <div v-else class="relative">
                <button
                  class="flex items-center gap-1 px-2 py-1 rounded-lg bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] text-xs hover:border-[var(--mobile-border-active)] active:opacity-80 transition-colors max-w-[clamp(100px,120px,160px)]"
                  @click="showDirDropdown = !showDirDropdown"
                >
                  <svg class="w-3 h-3 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                  <span class="truncate">{{ selectedDirLabel }}</span>
                  <svg class="w-2.5 h-2.5 flex-shrink-0 transition-transform duration-200" :class="{ 'rotate-180': showDirDropdown }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                  </svg>
                </button>
                <Transition name="dropdown">
                  <div v-if="showDirDropdown" class="absolute top-full right-0 mt-1 min-w-[clamp(140px,180px,220px)] max-h-[180px] overflow-y-auto bg-[var(--mobile-bg-tertiary)] border border-[var(--mobile-border)] rounded-lg shadow-[0_8px_24px_rgba(0,0,0,0.4)] z-30" @click.stop>
                    <button
                      v-for="dir in projectDirs"
                      :key="dir"
                      class="w-full text-left px-3 py-2 text-xs hover:bg-[var(--mobile-bg-elevated)] active:bg-[var(--mobile-bg-primary)] transition-colors flex items-center gap-2"
                      :class="dir === selectedDir ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-secondary)]'"
                      @click="selectedDir = dir; showDirDropdown = false"
                    >
                      <svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                      </svg>
                      <span class="truncate">{{ dir }}</span>
                      <svg v-if="dir === selectedDir" class="w-3 h-3 flex-shrink-0 ml-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                      </svg>
                    </button>
                  </div>
                </Transition>
              </div>
              <button
                class="p-1.5 rounded-lg bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] hover:bg-[color:color-mix(in_srgb,var(--mobile-accent)_30%,transparent)] active:scale-[0.98] transition-[background-color,transform] duration-150 flex-shrink-0"
                :disabled="!fileExplorerSessionId"
                @click="showFileExplorer = true"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                </svg>
              </button>
            </div>
          </div>

          <div class="space-y-4 flex-1 overflow-y-auto min-h-0">
            <!-- 可重复/不可重复属性（创建时设置；编辑时可改，改属性不重置执行状态） -->
            <RepeatableToggle v-model="form.repeatable" />
            <div class="flex-1 min-h-0 flex flex-col">
              <div class="flex items-center justify-between mb-1">
                <label class="text-[var(--mobile-text-muted)] text-sm">{{ t('mobile.toolbox.taskContent') }}</label>
                <button
                  v-if="!hasAiTemplate"
                  class="flex items-center gap-1 px-2 py-0.5 rounded-md text-xs font-medium bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] active:scale-[0.95] transition-transform duration-150"
                  @click="insertAiTemplate"
                >
                  <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                  {{ t('mobile.toolbox.insertAiTemplate') }}
                </button>
              </div>
              <textarea
                ref="contentTextarea"
                v-model="form.content"
                :placeholder="t('mobile.toolbox.taskContentPlaceholder')"
                rows="8"
                class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2.5 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-[color:color-mix(in_srgb,var(--mobile-accent)_50%,transparent)] transition-colors resize-none flex-1 min-h-[160px]"
              ></textarea>
            </div>
          </div>

          <div class="flex gap-3 mt-6 flex-shrink-0">
            <button
              class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-[color:color-mix(in_srgb,var(--mobile-accent)_40%,transparent)] active:opacity-80 transition-colors"
              @click="emit('close')"
            >
              {{ t('common.button.cancel') }}
            </button>
            <button
              class="flex-1 bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] py-2.5 rounded-xl font-medium hover:bg-[color:color-mix(in_srgb,var(--mobile-accent)_30%,transparent)] active:scale-[0.98] transition-transform duration-150"
              :class="{ 'opacity-50': !form.content }"
              :disabled="!form.content"
              @click="handleSave"
            >
              {{ t('common.button.save') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>

  <!-- File Explorer Dialog (层级高于 Edit Dialog) -->
  <Teleport to="body">
    <Transition name="center-modal">
      <div v-if="showFileExplorer && fileExplorerSessionId" class="fixed inset-0 z-[100] flex items-center justify-center p-[10%] mobile-ui">
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showFileExplorer = false"></div>
        <div class="relative w-full h-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl shadow-xl overflow-hidden flex flex-col modal-panel">
          <FileExplorer
            :session-id="fileExplorerSessionId"
            mode="emit"
            :title="effectiveDirLabel"
            @close="showFileExplorer = false"
          />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * TaskEditDialog - 预设任务新增/编辑弹窗
 *
 * 从 ToolboxView 抽取的共享组件，支持新增和编辑两种模式。
 * 包含工程目录选择和文件浏览功能。
 *
 * 当 lockedDir 传入时，目录自动锁定为该值且不可更改（终端视图场景），
 * 否则显示目录下拉菜单供用户选择（工具箱场景）。
 */

import { ref, computed, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import FileExplorer from '@/components/FileExplorer.vue'
import RepeatableToggle from '@/components/RepeatableToggle.vue'
import type { PresetTask } from '@/composables/model'

const props = defineProps<{
  /** 控制弹窗可见性 */
  visible: boolean
  /** 传入已有任务则为编辑模式，null 为新增模式 */
  task: PresetTask | null
  /** 是否已连接桌面端 */
  isConnected: boolean
  /** 可选的工程目录列表（工具箱场景使用） */
  projectDirs: string[]
  /** 当前活跃会话 ID（用于文件浏览 fallback） */
  activeSessionId: string
  /** 所有活跃会话列表 */
  activeSessions: any[]
  /** 所有会话配置列表 */
  sessionConfigs: any[]
  /**
   * 锁定的工作目录（终端视图场景）
   * 传入时：自动选中该目录，目录不可更改，隐藏下拉箭头
   * 不传时：显示目录下拉菜单供用户选择
   */
  lockedDir?: string
}>()

const emit = defineEmits<{
  /** 保存：新增时传 { content, repeatable }，编辑时传完整 PresetTask */
  save: [data: PresetTask | { content: string; repeatable: boolean }]
  close: []
}>()

const { t } = useI18n()

// ==================== AI 提示词模板 ====================

/** AI 编程提示词标准 4 要素模板 */
const AI_TEMPLATE = '目标：\n上下文：\n约束：\n完成条件：'

/** 检测内容中是否已包含模板要素 */
const hasAiTemplate = computed(() => {
  const c = form.value.content
  return c.includes('目标：') && c.includes('上下文：') && c.includes('约束：') && c.includes('完成条件：')
})

/** 插入 AI 提示词模板（仅当内容中尚未包含时） */
function insertAiTemplate() {
  if (hasAiTemplate.value) return
  const current = form.value.content.trim()
  form.value.content = current ? `${current}\n${AI_TEMPLATE}` : AI_TEMPLATE
}

// ==================== 表单状态 ====================

const contentTextarea = ref<HTMLTextAreaElement | null>(null)
const form = ref({ content: '', repeatable: true })
const showDirDropdown = ref(false)
const selectedDir = ref<string | null>(null)
const showFileExplorer = ref(false)

/** 实际生效的目录：lockedDir 优先，否则取用户选择 */
const effectiveDir = computed(() => props.lockedDir || selectedDir.value)

/** 是否有任何可用目录（决定是否显示目录区域） */
const effectiveProjectDir = computed(() => !!props.lockedDir || props.projectDirs.length > 0)

// 弹窗打开时初始化表单
// immediate：父组件可能通过 v-if 在 visible=true 时挂载本组件（懒加载场景），
// 此时 watch 不会因 visible 变化触发，需在挂载时立即初始化
watch(() => props.visible, (val) => {
  if (val) {
    if (props.task) {
      form.value = { content: props.task.content, repeatable: props.task.repeatable }
    } else {
      form.value = { content: '', repeatable: true }
    }
    // 非锁定模式才重置用户选择
    if (!props.lockedDir) {
      selectedDir.value = null
    }
    showDirDropdown.value = false
    showFileExplorer.value = false
    // 自动聚焦任务内容输入框
    nextTick(() => contentTextarea.value?.focus())
  }
}, { immediate: true })

// ==================== 目录选择 ====================

/** 目录短标签：仅显示最后一段路径 */
function dirLabel(dir: string | null): string {
  if (!dir) return t('mobile.toolbox.selectProject')
  const parts = dir.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || dir
}

/** 锁定目录的短标签 */
const lockedDirLabel = computed(() => dirLabel(props.lockedDir ?? null))

/** 用户选择目录的短标签 */
const selectedDirLabel = computed(() => dirLabel(selectedDir.value))

/** 用于 FileExplorer 标题的目录标签 */
const effectiveDirLabel = computed(() => dirLabel(effectiveDir.value))

/** 根据目录找到对应的活跃会话 ID */
const fileExplorerSessionId = computed(() => {
  const dir = effectiveDir.value
  if (!dir) {
    return props.activeSessionId
  }
  const matchedConfig = props.sessionConfigs.find((c: any) => c.working_dir === dir)
  if (!matchedConfig) return props.activeSessionId
  const session = props.activeSessions.find(
    (s: any) => s.config_id === matchedConfig.id || s.configId === matchedConfig.id
  )
  // 优先使用活跃会话 id；无可运行会话时回退到 config_id，桌面端文件 API 支持直接用 config_id 浏览
  return session?.id || matchedConfig.id
})

// ==================== 保存 ====================

function handleSave() {
  if (!form.value.content.trim()) return

  if (props.task) {
    // 编辑模式：返回更新后的完整 PresetTask
    emit('save', {
      ...props.task,
      content: form.value.content,
      repeatable: form.value.repeatable,
    } as PresetTask)
  } else {
    // 新增模式：返回表单数据（含可重复属性）
    emit('save', { content: form.value.content, repeatable: form.value.repeatable })
  }
}
</script>

<style scoped>
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
</style>
