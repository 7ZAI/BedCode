<template>
  <div
    class="terminal-input-bar z-10"
    :style="inputBarStyle"
  >
    <!-- 快捷键面板遮罩 - 点击终端区域关闭面板 -->
    <transition name="shortcuts-overlay">
      <div v-if="showShortcutsPanel && !props.isLandscape" class="shortcuts-overlay" @touchstart.prevent="closeShortcutsPanel" @mousedown.prevent="closeShortcutsPanel"></div>
    </transition>

    <!-- 快捷键面板 - 覆盖层，不影响终端高度 -->
    <transition name="shortcuts-slide">
      <div v-if="showShortcutsPanel && !props.isLandscape" ref="shortcutsPanelRef" class="shortcuts-panel" @mousedown.prevent>
      <!-- 轮播容器 -->
      <div
        ref="carouselRef"
        class="carousel-container"
        @touchstart="onTouchStart"
        @touchmove="onTouchMove"
        @touchend="onTouchEnd"
      >
        <div class="carousel-track" :style="trackStyle">
          <!-- 循环轮播：[-1]=最后一页克隆, [0]=第一页, [1]=第二页, [2]=第一页克隆 -->
          <!-- 位置 -1：最后一页（自定义命令）的克隆，用于从第一页右滑循环 -->
          <div class="carousel-slide">
            <div class="custom-commands-layout">
              <div class="custom-commands-grid">
                <button
                  v-for="cmd in displayCommands"
                  :key="'clone-end-' + cmd.id"
                  class="custom-cmd-btn"
                  :class="{ 'editing': isEditingCommands }"
                  @click="handleCustomCommandClick(cmd)"
                >
                  <span class="cmd-label">{{ cmd.command }}</span>
                  <transition name="delete-badge">
                    <button
                      v-if="isEditingCommands && !cmd.builtin"
                      class="cmd-delete-btn"
                      @click.stop="deleteCustomCommand(cmd.id)"
                    >
                      <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </transition>
                </button>
                <button
                  v-if="customCommands.length > 0"
                  class="custom-cmd-btn edit-toggle-btn"
                  @click="isEditingCommands = !isEditingCommands"
                >
                  <svg v-if="!isEditingCommands" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                  </svg>
                  <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </button>
                <button class="custom-cmd-btn add-cmd-btn" @click="isEditingCommands = false; showAddDialog = true">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M12 4v16m8-8H4" />
                  </svg>
                </button>
              </div>
            </div>
          </div>

          <!-- 位置 0：第一页（快捷键 + 方向键） -->
          <div class="carousel-slide">
            <div class="shortcuts-layout">
              <!-- 左侧：一般快捷键（不含 Enter/Del） -->
              <div class="shortcuts-left">
                <div class="shortcuts-grid">
                  <button
                    v-for="key in leftShortcuts"
                    :key="key.code"
                    class="shortcut-btn"
                    @click="handleShortcutClick(key.code)"
                  >
                    {{ key.label }}
                  </button>
                </div>
              </div>

              <!-- 中间：Enter/Del（右手拇指高频操作，靠近方向键） -->
              <div v-if="isEnterVisible || isDelVisible" class="shortcuts-center">
                <div class="action-keys-layout">
                  <button
                    v-if="isEnterVisible"
                    class="action-btn action-btn--enter"
                    @click="handleShortcutClick('enter')"
                  >
                    Enter
                  </button>
                  <button
                    v-if="isDelVisible"
                    class="action-btn action-btn--del"
                    @click="handleShortcutClick('backspace')"
                  >
                    Del
                  </button>
                </div>
              </div>

              <!-- 右侧：方向键（键盘布局） -->
              <div class="shortcuts-right">
                <div class="arrow-keys-layout">
                  <div class="arrow-row">
                    <div class="arrow-placeholder"></div>
                    <button
                      class="arrow-btn"
                      @click="handleShortcutClick('arrow_up')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 15l7-7 7 7" />
                      </svg>
                    </button>
                    <div class="arrow-placeholder"></div>
                  </div>
                  <div class="arrow-row">
                    <button
                      class="arrow-btn"
                      @click="handleShortcutClick('arrow_left')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M15 19l-7-7 7-7" />
                      </svg>
                    </button>
                    <button
                      class="arrow-btn arrow-down"
                      @click="handleShortcutClick('arrow_down')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M19 9l-7 7-7-7" />
                      </svg>
                    </button>
                    <button
                      class="arrow-btn"
                      @click="handleShortcutClick('arrow_right')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M9 5l7 7-7 7" />
                      </svg>
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- 位置 1：第二页（自定义命令） -->
          <div class="carousel-slide">
            <div class="custom-commands-layout">
              <div class="custom-commands-grid">
                <button
                  v-for="cmd in displayCommands"
                  :key="cmd.id"
                  class="custom-cmd-btn"
                  :class="{ 'editing': isEditingCommands }"
                  @click="handleCustomCommandClick(cmd)"
                >
                  <span class="cmd-label">{{ cmd.command }}</span>
                  <transition name="delete-badge">
                    <button
                      v-if="isEditingCommands && !cmd.builtin"
                      class="cmd-delete-btn"
                      @click.stop="deleteCustomCommand(cmd.id)"
                    >
                      <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </transition>
                </button>

                <!-- 编辑/完成按钮 -->
                <button
                  v-if="customCommands.length > 0"
                  class="custom-cmd-btn edit-toggle-btn"
                  @click="isEditingCommands = !isEditingCommands"
                >
                  <svg v-if="!isEditingCommands" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                  </svg>
                  <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </button>

                <!-- 添加按钮 -->
                <button class="custom-cmd-btn add-cmd-btn" @click="isEditingCommands = false; showAddDialog = true">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M12 4v16m8-8H4" />
                  </svg>
                </button>
              </div>
            </div>
          </div>

          <!-- 位置 2：第一页（快捷键 + 方向键）的克隆，用于从最后一页左滑循环 -->
          <div class="carousel-slide">
            <div class="shortcuts-layout">
              <div class="shortcuts-left">
                <div class="shortcuts-grid">
                  <button
                    v-for="key in leftShortcuts"
                    :key="'clone-start-' + key.code"
                    class="shortcut-btn"
                    @click="handleShortcutClick(key.code)"
                  >
                    {{ key.label }}
                  </button>
                </div>
              </div>
              <div v-if="isEnterVisible || isDelVisible" class="shortcuts-center">
                <div class="action-keys-layout">
                  <button
                    v-if="isEnterVisible"
                    class="action-btn action-btn--enter"
                    @click="handleShortcutClick('enter')"
                  >
                    Enter
                  </button>
                  <button
                    v-if="isDelVisible"
                    class="action-btn action-btn--del"
                    @click="handleShortcutClick('backspace')"
                  >
                    Del
                  </button>
                </div>
              </div>
              <div class="shortcuts-right">
                <div class="arrow-keys-layout">
                  <div class="arrow-row">
                    <div class="arrow-placeholder"></div>
                    <button
                      class="arrow-btn"
                      @click="handleShortcutClick('arrow_up')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 15l7-7 7 7" />
                      </svg>
                    </button>
                    <div class="arrow-placeholder"></div>
                  </div>
                  <div class="arrow-row">
                    <button
                      class="arrow-btn"
                      @click="handleShortcutClick('arrow_left')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M15 19l-7-7 7-7" />
                      </svg>
                    </button>
                    <button
                      class="arrow-btn arrow-down"
                      @click="handleShortcutClick('arrow_down')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M19 9l-7 7-7-7" />
                      </svg>
                    </button>
                    <button
                      class="arrow-btn"
                      @click="handleShortcutClick('arrow_right')"
                    >
                      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M9 5l7 7-7 7" />
                      </svg>
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 页码指示器 -->
      <div class="carousel-dots">
        <div
          class="dot"
          :class="{ active: displaySlide === 0 }"
          @click="goToSlide(0)"
        ></div>
        <div
          class="dot"
          :class="{ active: displaySlide === 1 }"
          @click="goToSlide(1)"
        ></div>
      </div>
    </div>
    </transition>

    <!-- 添加自定义命令弹窗 -->
    <Teleport to="body">
      <div v-if="showAddDialog" class="dialog-overlay mobile-ui" @click.self="showAddDialog = false">
        <div class="dialog-box">
          <div class="dialog-title">{{ t('mobile.input.commandTitle') }}</div>
          <input
            ref="cmdInputRef"
            v-model="newCommand"
            class="dialog-input"
            :placeholder="t('mobile.input.commandPlaceholder')"
            @keyup.enter="addCustomCommand"
          />
          <div class="dialog-actions">
            <button class="dialog-btn cancel" @click="showAddDialog = false">{{ t('common.button.cancel') }}</button>
            <button class="dialog-btn confirm" :disabled="!newCommand.trim()" @click="addCustomCommand">{{ t('common.button.confirm') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- 快捷键条 - 常驻显示最常用的快捷键和自定义命令 -->
    <div v-if="quickBarItems.length > 0" class="quick-bar" @mousedown.prevent>
      <button
        v-for="item in quickBarItems"
        :key="item.type + '-' + item.key"
        class="quick-bar-btn"
        :class="quickBarClass(item.category)"
        @click="handleQuickBarClick(item)"
      >
        {{ item.label }}
      </button>
    </div>

    <!-- 输入区域 -->
    <div class="input-area">
      <!-- `/` 命令补全弹层：输入框上方，点选即填充输入框（复用 agentPresets 本地数据，零延迟） -->
      <transition name="completion-fade">
        <div v-if="showCompletion" class="completion-panel" @mousedown.prevent>
          <button
            v-for="cmd in completionItems"
            :key="cmd"
            class="completion-item"
            @click="applyCompletion(cmd)"
          >
            <span class="completion-cmd">{{ cmd }}</span>
          </button>
        </div>
      </transition>
      <div class="input-box" :class="{ 'input-box--expanded': isInputFocused }">
        <!-- 输入框：占满整行宽度 -->
        <textarea
          ref="inputRef"
          v-model="inputText"
          class="input-field"
          :class="{ 'input-field--expanded': isInputFocused }"
          :placeholder="placeholder"
          :disabled="disabled"
          rows="1"
          @focus="handleFocus"
          @blur="handleBlur"
          @input="adjustTextareaHeight"
        ></textarea>

        <!-- 操作按钮行：输入框下方，不挤占输入宽度 -->
        <div class="action-row">
          <div class="action-row-spacer"></div>

          <button
            class="inline-btn toggle-btn"
            :class="showShortcutsPanel ? 'toggle-active' : 'toggle-inactive'"
            @mousedown.prevent="toggleShortcuts"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
            </svg>
          </button>

          <button
            class="inline-btn send-btn"
            :disabled="!canSubmit"
            @click="handleSubmit"
          >
            <!-- 实体上箭头：发送语义，填充图标 + 放大尺寸提升辨识度 -->
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 4l8 9h-4.5v7h-7v-7H4z" />
            </svg>
          </button>

          <button
            class="inline-btn execute-btn"
            :disabled="!canSubmit"
            @click="handleExecute"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, inject, onMounted, nextTick, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useInputAssistantStore, type QuickCommand } from '@/stores/inputAssistant'
import type { QuickBarItem } from '@/stores/inputAssistant'
import { filterPresetCommands, getAllPresetCommandTexts } from '@/config/agentPresets'
import { useToast } from '@/composables/useToast'

// ==================== Types ====================

/** 面板命令项：用户自定义命令 + 命令预设（builtin）的并集 */
type PanelCommand = QuickCommand

// ==================== Props ====================

const props = withDefaults(defineProps<{
  disabled?: boolean
  isConnected?: boolean
  placeholder?: string
  isLandscape?: boolean
  /** 侧栏「插入引用」待填入的路径（消费后 emit ref-consumed） */
  pendingRef?: string | null
}>(), {
  disabled: false,
  isConnected: false,
  placeholder: '',
  isLandscape: false,
  pendingRef: null,
})

// ==================== Emits ====================

const emit = defineEmits<{
  submit: [text: string]
  execute: [text: string]
  specialKey: [key: string]
  /** 快捷键面板展开/收起时通知终端，传入面板高度用于偏移 */
  shortcutsPanelToggle: [height: number]
  /** pendingRef 已被填入输入框，通知父组件复位 */
  refConsumed: []
}>()

// ==================== Safe Area ====================

const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar: number }>>('safeArea')

const inputBarStyle = computed(() => {
  const jsBottom = safeArea?.value?.navigationBar || safeArea?.value?.bottom || 0
  return {
    paddingBottom: `${jsBottom}px`,
  }
})

// ==================== State ====================

const assistStore = useInputAssistantStore()
const toast = useToast()
const { t } = useI18n()
const inputRef = ref<HTMLTextAreaElement | null>(null)
const shortcutsPanelRef = ref<HTMLElement | null>(null)
const inputText = ref('')
const showShortcutsPanel = ref(false)
const isInputFocused = ref(false)
const showAddDialog = ref(false)
const newCommand = ref('')
const cmdInputRef = ref<HTMLInputElement | null>(null)

// ==================== Carousel State ====================

const TOTAL_SLIDES = 2
const carouselRef = ref<HTMLElement | null>(null)
const currentSlide = ref(0)
const touchStartX = ref(0)
const touchStartY = ref(0)
const touchDeltaX = ref(0)
const isSwiping = ref(false)
// 循环跳转中间状态：跳转时临时禁用 transition，跳转完成后恢复
const isLooping = ref(false)

/** 用于点指示器的规范化 slide 索引（将循环过渡中的 -1/2 映射回真实范围） */
const displaySlide = computed(() => {
  return ((currentSlide.value % TOTAL_SLIDES) + TOTAL_SLIDES) % TOTAL_SLIDES
})

// 轨道布局：[克隆末页][-1] [第一页][0] [第二页][1] [克隆首页][2]
// 真实 slide 从偏移 1 开始，需要 +1 补偿
const trackStyle = computed(() => ({
  transform: `translateX(${-(currentSlide.value + 1) * 100 + touchDeltaX.value}%)`,
  transition: isSwiping.value || isLooping.value ? 'none' : 'transform 0.3s ease',
}))

// ==================== `/` 命令补全 ====================
// 与 agent 内部补全同构（前缀匹配），但数据来自本地预设（agentPresets），零延迟；
// 候选为四套 Agent CLI 预设命令的合集（去重），generic 会话同样可用

const completionItems = computed(() => {
  if (!inputText.value.startsWith('/')) return []
  return filterPresetCommands(getAllPresetCommandTexts(), inputText.value)
})

/** 点选后关闭弹层（对齐 agent 内部补全行为）；下次输入时自动恢复 */
const completionDismissed = ref(false)

// flush:sync —— applyCompletion 写入后同步复位标记，保证点选必然关闭弹层
watch(inputText, () => {
  completionDismissed.value = false
  // 用户开始打字时收起快捷键面板，避免补全弹层与面板重叠遮挡
  if (showShortcutsPanel.value) {
    showShortcutsPanel.value = false
    emit('shortcutsPanelToggle', 0)
  }
}, { flush: 'sync' })

const showCompletion = computed(() =>
  isInputFocused.value && !completionDismissed.value && completionItems.value.length > 0
)

/** 点选补全项：整体填充输入框并保持焦点，由用户决定补全/发送 */
function applyCompletion(command: string) {
  inputText.value = command
  // 覆盖 sync watcher 的复位：点选后弹层关闭，等下一次真实输入再出现
  completionDismissed.value = true
  nextTick(() => {
    adjustTextareaHeight()
    inputRef.value?.focus()
  })
}

// ==================== Pending Ref ====================

// 侧栏「插入引用」：把 @路径 填入输入框（已有内容时补空格分隔）并聚焦，便于继续输入
watch(() => props.pendingRef, (path) => {
  if (!path) return
  const refText = `@${path}`
  const text = inputText.value
  inputText.value = text && !text.endsWith(' ') ? `${text} ${refText}` : `${text}${refText}`
  emit('refConsumed')
  nextTick(() => {
    adjustTextareaHeight()
    inputRef.value?.focus()
  })
})

// ==================== Custom Commands ====================

const customCommands = ref<QuickCommand[]>([])
const isEditingCommands = ref(false)

/** 面板命令 = 命令预设（builtin，在前） + 用户自定义命令 */
const displayCommands = computed(() => [...assistStore.presetCommands, ...customCommands.value])

// 从 Tauri settings（JSON 文件）持久化加载自定义命令
async function loadCustomCommands() {
  try {
    const settings = await invoke<{ key: string; value: string }[]>('get_all_db_settings_mobile')
    const found = settings?.find(s => s.key === 'custom_commands')
    if (found?.value) {
      customCommands.value = (JSON.parse(found.value) as Partial<QuickCommand>[]).map(c => ({
        id: c.id || Date.now().toString(),
        command: c.command || '',
        // 旧数据无 mode/builtin：默认执行模式、非内置
        mode: c.mode || 'execute',
        builtin: c.builtin ?? false,
      }))
    }
  } catch {
    // 首次加载或非移动端环境，使用空列表
    customCommands.value = []
  }
}

// 持久化保存自定义命令
async function saveCustomCommands() {
  try {
    await invoke('set_db_setting_mobile', {
      key: 'custom_commands',
      value: JSON.stringify(customCommands.value),
    })
  } catch (e) {
    console.error('[TerminalInputBar] Failed to save custom commands:', e)
  }
}

function addCustomCommand() {
  const cmd = newCommand.value.trim()
  if (!cmd) return
  customCommands.value.push({
    id: Date.now().toString(),
    command: cmd,
    mode: 'execute',
    builtin: false,
  })
  saveCustomCommands()
  newCommand.value = ''
  showAddDialog.value = false
}

function handleCustomCommandClick(cmd: PanelCommand) {
  // 编辑模式下点击不执行命令
  if (isEditingCommands.value) return
  assistStore.recordCustomCommand(cmd.id)
  // 发送模式（skills 类补全场景）：文本不带回车发到终端输入行；否则执行（文本 + Enter）
  if (cmd.mode === 'send') {
    emit('submit', cmd.command)
  } else {
    emit('execute', cmd.command)
  }
}

function deleteCustomCommand(id: string) {
  customCommands.value = customCommands.value.filter(c => c.id !== id)
  saveCustomCommands()
  // 删完所有命令后自动退出编辑模式
  if (customCommands.value.length === 0) {
    isEditingCommands.value = false
  }
}

// ==================== Shortcuts Data ====================

// 从 store 读取动态快捷键列表（不含 Enter/Del，它们由中间区域独立渲染）
const leftShortcuts = computed(() => assistStore.visiblePanelShortcuts)

// Enter/Del 可见性：由配置控制，默认显示
const isEnterVisible = computed(() => assistStore.shortcutConfig.find(s => s.code === 'enter')?.visible ?? true)
const isDelVisible = computed(() => assistStore.shortcutConfig.find(s => s.code === 'backspace')?.visible ?? true)

// ==================== Carousel Methods ====================

function goToSlide(index: number) {
  currentSlide.value = ((index % TOTAL_SLIDES) + TOTAL_SLIDES) % TOTAL_SLIDES
}

function onTouchStart(e: TouchEvent) {
  touchStartX.value = e.touches[0].clientX
  touchStartY.value = e.touches[0].clientY
  touchDeltaX.value = 0
  isSwiping.value = true
}

function onTouchMove(e: TouchEvent) {
  const deltaX = e.touches[0].clientX - touchStartX.value
  const deltaY = e.touches[0].clientY - touchStartY.value

  // 水平滑动距离大于垂直时才处理，避免影响页面滚动
  if (Math.abs(deltaX) > Math.abs(deltaY) && carouselRef.value) {
    const width = carouselRef.value.offsetWidth
    touchDeltaX.value = (deltaX / width) * 100
  }
}

function onTouchEnd() {
  isSwiping.value = false
  const threshold = 20

  if (touchDeltaX.value < -threshold) {
    // 左滑 → 下一页（末尾循环到第一页）
    slideToNext()
  } else if (touchDeltaX.value > threshold) {
    // 右滑 → 上一页（开头循环到最后一页）
    slideToPrev()
  } else {
    touchDeltaX.value = 0
  }
}

/** 左滑切换到下一页，末尾循环到第一页 */
function slideToNext() {
  if (currentSlide.value < TOTAL_SLIDES - 1) {
    currentSlide.value++
    touchDeltaX.value = 0
  } else {
    // 已在最后一页：动画滑到位置 2（首页克隆），然后无动画跳回位置 0
    currentSlide.value = TOTAL_SLIDES
    touchDeltaX.value = 0
    nextTick(() => {
      isLooping.value = true
      currentSlide.value = 0
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          isLooping.value = false
        })
      })
    })
  }
}

/** 右滑切换到上一页，开头循环到最后一页 */
function slideToPrev() {
  if (currentSlide.value > 0) {
    currentSlide.value--
    touchDeltaX.value = 0
  } else {
    // 已在第一页：动画滑到位置 -1（末页克隆），然后无动画跳回位置 1
    currentSlide.value = -1
    touchDeltaX.value = 0
    nextTick(() => {
      isLooping.value = true
      currentSlide.value = TOTAL_SLIDES - 1
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          isLooping.value = false
        })
      })
    })
  }
}

// ==================== Computed ====================

const canSubmit = computed(() => {
  return inputText.value.trim().length > 0 && !props.disabled
})

// ==================== Methods ====================

function toggleShortcuts() {
  // 横屏高度有限，面板不渲染：提示用户而非静默无响应
  if (props.isLandscape) {
    toast.warning(t('mobile.input.shortcutsLandscapeUnavailable'))
    return
  }
  showShortcutsPanel.value = !showShortcutsPanel.value
  if (showShortcutsPanel.value) {
    // 面板渲染前确认命令列表构成（预设 + 自定义），排查第二页缺失问题
    console.log('[TerminalInputBar] 面板打开：displayCommands =', displayCommands.value.length,
      '（预设', assistStore.presetCommands.length, '+ 自定义', customCommands.value.length, '）',
      displayCommands.value.slice(0, 3).map(c => c.command))
    // 面板渲染后测量高度并通知终端，同时计算左侧网格列数
    nextTick(() => {
      const h = shortcutsPanelRef.value?.offsetHeight || 0
      emit('shortcutsPanelToggle', h)
    })
  } else {
    // 立即通知终端收起，xterm 的 transition 会与面板 leave 动画同步
    // 两者都是 0.25s cubic-bezier(0.4, 0, 0.2, 1)，视觉上同步下落
    emit('shortcutsPanelToggle', 0)
  }
}

function closeShortcutsPanel() {
  if (!showShortcutsPanel.value) return
  showShortcutsPanel.value = false
  emit('shortcutsPanelToggle', 0)
}

function handleSubmit() {
  const text = inputText.value.trim()
  if (!text) return
  emit('submit', text)
  inputText.value = ''
  if (inputRef.value) {
    inputRef.value.style.height = 'auto'
  }
}

function handleExecute() {
  const text = inputText.value.trim()
  if (!text) return
  emit('execute', text)
  inputText.value = ''
  if (inputRef.value) {
    inputRef.value.style.height = 'auto'
  }
}

function handleShortcutClick(code: string) {
  assistStore.recordShortcut(code)
  emit('specialKey', code)
}

// ==================== Quick Bar ====================

/// 快捷键条项目：合并快捷键和快捷命令，按使用频次排序
const quickBarItems = computed(() => assistStore.getQuickBarItems(displayCommands.value))

/// 快捷键条按钮点击处理
function handleQuickBarClick(item: QuickBarItem) {
  if (item.type === 'shortcut') {
    assistStore.recordShortcut(item.key)
    emit('specialKey', item.key)
  } else {
    // 快捷命令：找到对应命令项，按模式分发（发送 = 不带回车，执行 = 文本 + Enter）
    const cmd = displayCommands.value.find(c => c.id === item.key)
    if (cmd) {
      assistStore.recordCustomCommand(cmd.id)
      if (cmd.mode === 'send') {
        emit('submit', cmd.command)
      } else {
        emit('execute', cmd.command)
      }
    }
  }
}

/// 根据 category 返回 quick bar 按钮的样式类
function quickBarClass(category: string): string {
  const map: Record<string, string> = {
    enter: 'quick-bar-enter',
    del: 'quick-bar-del',
    arrow: 'quick-bar-arrow',
    shortcut: 'quick-bar-shortcut',
    custom: 'quick-bar-custom',
  }
  return map[category] || 'quick-bar-shortcut'
}

function handleFocus() {
  isInputFocused.value = true
  // 延迟调整高度，等键盘弹出后再计算
  setTimeout(() => {
    adjustTextareaHeight()
  }, 300)
}

function handleBlur() {
  isInputFocused.value = false
  // 延迟收缩，等键盘收起动画完成后再调整高度，避免跳变
  setTimeout(() => {
    if (!inputText.value.trim() && inputRef.value) {
      inputRef.value.style.height = 'auto'
    }
  }, 300)
}

function adjustTextareaHeight() {
  const textarea = inputRef.value
  if (!textarea) return
  textarea.style.height = 'auto'
  const lineHeight = parseFloat(getComputedStyle(textarea).lineHeight) || 21
  // 聚焦时最小 3 行，失焦时最小 1 行；最大 6 行，超过后滚动
  const minLines = isInputFocused.value ? 3 : 1
  const maxLines = 6
  const minHeight = lineHeight * minLines
  const maxHeight = lineHeight * maxLines
  const newHeight = Math.max(minHeight, Math.min(textarea.scrollHeight, maxHeight))
  textarea.style.height = `${newHeight}px`
  // 超过最大行数时滚动到底部
  textarea.scrollTop = textarea.scrollHeight
}

// 弹窗打开时自动聚焦输入框
watch(showAddDialog, (val) => {
  if (val) {
    nextTick(() => {
      cmdInputRef.value?.focus()
    })
  }
})

// ==================== Lifecycle ====================

onMounted(() => {
  loadCustomCommands()
})
</script>

<style scoped>
.terminal-input-bar {
  flex-shrink: 0;
  background: var(--mobile-bg-secondary);
  backdrop-filter: blur(20px);
  border-top: 1px solid var(--mobile-border);
  padding: 0.5rem 1rem;
  position: relative;
  /* paddingBottom 由 JS 动态设置（导航栏安全区域），不使用 CSS transition
   * padding 动画触发布局重排，与终端 xterm 重影问题同理 */
  /* 响应式快捷键尺寸：使用 clamp + vw 实现自适应 */
  --shortcut-btn-h: clamp(2rem, 8vw, 2.5rem);
  --shortcut-font: clamp(0.65rem, 2.6vw, 0.8rem);
  --quickbar-btn-h: clamp(1.5rem, 6vw, 2rem);
  --quickbar-font: clamp(0.6rem, 2.4vw, 0.75rem);
  --action-btn-w: clamp(2.75rem, 10vw, 3.25rem);
  --shortcut-min-w: clamp(2.25rem, 8.5vw, 2.75rem);
}

/* ==================== Quick Bar ==================== */

.quick-bar {
  display: flex;
  flex-wrap: nowrap;
  gap: 0.375rem;
  padding-bottom: 0.375rem;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
  -webkit-overflow-scrolling: touch;
  position: relative;
  z-index: 40;
  /* RTL 布局：首项渲染在右侧，新项目从左侧添加，右对齐方便拇指操作 */
  direction: rtl;
}

.quick-bar::-webkit-scrollbar {
  display: none;
  width: 0;
}

.quick-bar-btn {
  height: var(--quickbar-btn-h);
  padding: 0 0.5rem;
  font-size: var(--quickbar-font);
  font-weight: 500;
  border-radius: 0.375rem;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid;
  /* 父容器 RTL 布局，按钮文本保持 LTR */
  direction: ltr;
}

.quick-bar-shortcut {
  background: var(--mobile-shortcut-bg);
  border-color: var(--mobile-shortcut-border);
  color: var(--mobile-shortcut-color);
}

.quick-bar-shortcut:active {
  transform: scale(0.93);
  background: var(--mobile-shortcut-active-bg);
}

.quick-bar-custom {
  background: var(--mobile-custom-cmd-bg);
  border-color: var(--mobile-custom-cmd-border);
  color: var(--mobile-custom-cmd-color);
}

.quick-bar-custom:active {
  transform: scale(0.93);
  background: var(--mobile-custom-cmd-active-bg);
}

.quick-bar-enter {
  background: var(--mobile-confirm-bg);
  border-color: var(--mobile-confirm-border);
  color: var(--mobile-confirm-color);
}

.quick-bar-enter:active {
  transform: scale(0.93);
  background: var(--mobile-confirm-bg);
  filter: brightness(1.2);
}

.quick-bar-del {
  background: var(--mobile-danger-bg);
  border-color: var(--mobile-danger-border);
  color: var(--mobile-danger-color);
}

.quick-bar-del:active {
  transform: scale(0.93);
  background: var(--mobile-danger-bg);
  filter: brightness(1.2);
}

.quick-bar-arrow {
  background: var(--mobile-arrow-bg);
  border-color: var(--mobile-arrow-border);
  color: var(--mobile-arrow-color);
}

.quick-bar-arrow:active {
  transform: scale(0.93);
  background: var(--mobile-arrow-active-bg);
}

.input-area {
  display: flex;
  align-items: flex-end;
  position: relative;
  z-index: 40;
}

.input-box {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 1rem;
  padding: 0.5rem 0.625rem 0.375rem;
  transition: border-color 0.2s ease, box-shadow 0.2s ease, border-radius 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  min-height: 2.5rem;
}

.input-box:focus-within {
  border-color: var(--mobile-accent);
  box-shadow: 0 0 0 2px var(--mobile-accent-muted);
}

/* 聚焦时输入框圆角微调 */
.input-box--expanded {
  border-radius: 0.875rem;
}

/* 操作按钮行：左对齐 toggle，右对齐 send/execute */
.action-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding-top: 0.375rem;
}

.action-row-spacer {
  flex: 1;
}

/* 操作按钮：圆角矩形 + clamp 流式尺寸，与快捷键面板按钮同风格（触控目标 ≥ 44px） */
.inline-btn {
  width: clamp(2.75rem, 9vw, 3rem);
  height: clamp(2.75rem, 9vw, 3rem);
  display: flex;
  align-items: center;
  justify-content: center;
  /* 圆形按钮：宽高相等 + 50% 圆角 */
  border-radius: 50%;
  border: 1px solid;
  cursor: pointer;
  transition: all 0.15s ease;
  flex-shrink: 0;
  padding: 0;
}

.toggle-btn {
  background: var(--mobile-bg-elevated);
  border-color: var(--mobile-border);
  color: var(--mobile-text-muted);
}

.toggle-btn:active {
  transform: scale(0.93);
  background: var(--mobile-bg-secondary);
}

.toggle-active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

.toggle-inactive {
  color: var(--mobile-text-muted);
}

.input-field {
  width: 100%;
  background: transparent;
  border: none;
  outline: none;
  color: var(--mobile-text-primary);
  font-size: var(--font-size-base);
  font-family: inherit;
  resize: none;
  line-height: 1.5;
  min-height: 1.5rem;
  /* 高度变化过渡：聚焦展开/失焦收缩时平滑动画 */
  transition: min-height 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  /* 6 行最大高度，超过后滚动 */
  max-height: calc(1.5em * 6);
  overflow-y: auto;
  scrollbar-width: none;
}

.input-field::-webkit-scrollbar {
  display: none;
  width: 0;
}

/* 聚焦时 textarea 展开到 3 行最小高度 */
.input-field--expanded {
  min-height: calc(1.5em * 3);
}

.input-field::placeholder {
  color: var(--mobile-input-placeholder);
}

.send-btn {
  background: var(--mobile-send-bg);
  border-color: var(--mobile-send-border);
  color: var(--mobile-send-color);
}

.send-btn:active:not(:disabled) {
  transform: scale(0.93);
  background: var(--mobile-send-active-bg);
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.execute-btn {
  /* 黑底白图标：终端「执行」语义（用户指定，跨主题稳定） */
  background: #0a0a0f;
  border-color: #0a0a0f;
  color: #ffffff;
}

.execute-btn:active:not(:disabled) {
  transform: scale(0.93);
  background: #0a0a0f;
  filter: brightness(1.3);
}

.execute-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ==================== `/` Command Completion ==================== */

.completion-panel {
  position: absolute;
  left: 0;
  right: 0;
  bottom: calc(100% + 0.5rem);
  z-index: 50;
  background: var(--mobile-bg-card);
  border: 1px solid var(--mobile-border);
  border-radius: 0.875rem;
  box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.15);
  overflow-y: auto;
  /* 紧凑布局：面板内边距 + 圆角块式项（分隔线已移除） */
  padding: 0.25rem;
  max-height: clamp(7rem, 26vh, 12rem);
  scrollbar-width: none;
  -webkit-overflow-scrolling: touch;
}

.completion-panel::-webkit-scrollbar {
  display: none;
  width: 0;
}

.completion-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  width: 100%;
  /* 触控下限保持 44px，上限收紧让列表更紧凑 */
  height: clamp(2.75rem, 8vw, 2.875rem);
  padding: 0 0.75rem;
  background: transparent;
  border: none;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.completion-item:active {
  background: var(--mobile-accent-muted);
}

.completion-cmd {
  font-family: 'Courier New', monospace;
  font-size: var(--font-size-sm);
  color: var(--mobile-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.completion-fade-enter-active,
.completion-fade-leave-active {
  transition: opacity 0.15s ease;
}

.completion-fade-enter-from,
.completion-fade-leave-to {
  opacity: 0;
}

/* ==================== Carousel ==================== */

.shortcuts-panel {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 100%;
  border-bottom: 1px solid var(--mobile-border);
  padding: 0.5rem 0.75rem 0.375rem;
  background: var(--mobile-bg-secondary);
  backdrop-filter: blur(20px);
  box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.15);
  z-index: 30;
  max-width: 100vw;
  overflow: visible;
  box-sizing: border-box;
  /*
   * 固定高度 = padding + 2行按钮 + dots
   * padding-top: 0.5rem, padding-bottom: 0.375rem
   * 内容区: 2*btn-h + gap(0.375rem) + dots(padding-top 0.375rem + dot 0.375rem)
   */
  --panel-h: calc(0.5rem + 2 * var(--shortcut-btn-h) + 0.375rem + 0.75rem + 0.375rem);
  height: var(--panel-h);
  min-height: var(--panel-h);
  max-height: var(--panel-h);
}

/* 快捷键面板遮罩 - 覆盖终端区域，点击关闭面板 */
.shortcuts-overlay {
  position: fixed;
  inset: 0;
  z-index: 29;
}

.shortcuts-overlay-enter-active,
.shortcuts-overlay-leave-active {
  transition: opacity 0.2s ease;
}

.shortcuts-overlay-enter-from,
.shortcuts-overlay-leave-to {
  opacity: 0;
}

/* 快捷键面板滑动动画 - 从下往上展开/收起 */
.shortcuts-slide-enter-active,
.shortcuts-slide-leave-active {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1),
              opacity 0.2s ease;
}

.shortcuts-slide-enter-from,
.shortcuts-slide-leave-to {
  transform: translateY(100%);
  opacity: 0;
}

.shortcuts-slide-enter-to,
.shortcuts-slide-leave-from {
  transform: translateY(0);
  opacity: 1;
}

.carousel-container {
  overflow-x: hidden;
  overflow-y: visible;
  max-width: 100%;
  height: calc(2 * var(--shortcut-btn-h) + 0.375rem);
}

.carousel-track {
  display: flex;
  will-change: transform;
  /* 固定高度，防止被内容撑开 */
  height: calc(2 * var(--shortcut-btn-h) + 0.375rem);
}

/* 每页轮播固定宽高，绝不超出 */
.carousel-slide {
  width: 100%;
  height: calc(2 * var(--shortcut-btn-h) + 0.375rem);
  overflow: visible;
  box-sizing: border-box;
  flex-shrink: 0;
}

.carousel-dots {
  display: flex;
  justify-content: center;
  gap: 0.375rem;
  padding-top: 0.375rem;
}

.dot {
  width: 0.375rem;
  height: 0.375rem;
  border-radius: 9999px;
  background: var(--mobile-border);
  transition: all 0.3s ease;
  cursor: pointer;
}

.dot.active {
  background: var(--mobile-accent);
  width: 1rem;
}

/* ==================== Shortcuts (Slide 1) ==================== */

/* grid 布局：左侧自适应 | 中间固定 | 右侧固定 */
.shortcuts-layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  gap: 0.5rem;
  align-items: stretch;
  /* 固定 2 行高度 */
  height: calc(2 * var(--shortcut-btn-h) + 0.375rem);
  width: 100%;
  max-width: 100%;
  overflow: visible;
  box-sizing: border-box;
}

/* 左侧：占满剩余宽度，固定2行高度，超出可滚动 */
.shortcuts-left {
  min-width: 0;
  height: calc(2 * var(--shortcut-btn-h) + 0.375rem);
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
  -webkit-overflow-scrolling: touch;
  box-sizing: border-box;
}

.shortcuts-left::-webkit-scrollbar {
  display: none;
  width: 0;
}

.shortcuts-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
}

.shortcut-btn {
  height: var(--shortcut-btn-h);
  padding: 0 0.5rem;
  white-space: nowrap;
  min-width: var(--shortcut-min-w);
  background: var(--mobile-shortcut-bg);
  border: 1px solid var(--mobile-shortcut-border);
  color: var(--mobile-shortcut-color);
  font-size: var(--shortcut-font);
  font-weight: 500;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.shortcut-btn:active {
  transform: scale(0.95);
  background: var(--mobile-shortcut-active-bg);
}

/* 中间：Enter/Del，固定宽度 */
.shortcuts-center {
  display: flex;
  align-items: center;
}

.action-keys-layout {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.action-btn {
  width: var(--action-btn-w);
  height: var(--shortcut-btn-h);
  font-size: var(--shortcut-font);
  font-weight: 600;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid;
}

.action-btn--enter {
  background: var(--mobile-confirm-bg);
  border-color: var(--mobile-confirm-border);
  color: var(--mobile-confirm-color);
}

.action-btn--enter:active {
  transform: scale(0.95);
  filter: brightness(1.2);
}

.action-btn--del {
  background: var(--mobile-danger-bg);
  border-color: var(--mobile-danger-border);
  color: var(--mobile-danger-color);
}

.action-btn--del:active {
  transform: scale(0.95);
  filter: brightness(1.2);
}

/* 右侧：方向键，固定宽度 */
.shortcuts-right {
}

.arrow-keys-layout {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.arrow-row {
  display: flex;
  gap: 0.375rem;
  justify-content: center;
}

.arrow-placeholder {
  width: 2.5rem;
  height: var(--shortcut-btn-h);
}

.arrow-btn {
  width: 2.5rem;
  height: var(--shortcut-btn-h);
  background: var(--mobile-arrow-bg);
  border: 1px solid var(--mobile-arrow-border);
  color: var(--mobile-arrow-color);
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.arrow-btn:active {
  transform: scale(0.9);
  background: var(--mobile-arrow-active-bg);
}

.arrow-icon {
  width: 0.875rem;
  height: 0.875rem;
}

/* ==================== Custom Commands (Slide 2) ==================== */

.custom-commands-layout {
  width: 100%;
  height: calc(2 * var(--shortcut-btn-h) + 0.375rem);
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;
  -webkit-overflow-scrolling: touch;
  box-sizing: border-box;
}

.custom-commands-layout::-webkit-scrollbar {
  display: none;
  width: 0;
}

.custom-commands-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
}

.custom-cmd-btn {
  padding: 0 0.5rem;
  white-space: nowrap;
  min-width: var(--shortcut-min-w);
  height: var(--shortcut-btn-h);
  background: var(--mobile-custom-cmd-bg);
  border: 1px solid var(--mobile-custom-cmd-border);
  color: var(--mobile-custom-cmd-color);
  font-size: var(--shortcut-font);
  font-weight: 500;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
  padding: 0 0.25rem;
}

.custom-cmd-btn:active {
  transform: scale(0.95);
  background: var(--mobile-custom-cmd-active-bg);
}

.cmd-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

/* 编辑模式下按钮抖动提示 */
.custom-cmd-btn.editing {
  animation: wiggle 0.3s ease-in-out;
  border-color: var(--mobile-danger-border);
  background: var(--mobile-danger-bg);
  color: var(--mobile-danger-color);
}

@keyframes wiggle {
  0%, 100% { transform: rotate(0deg); }
  25% { transform: rotate(-2deg); }
  75% { transform: rotate(2deg); }
}

.cmd-delete-btn {
  position: absolute;
  top: -0.25rem;
  right: -0.25rem;
  width: 1rem;
  height: 1rem;
  background: var(--mobile-danger-solid-bg);
  border: none;
  border-radius: 9999px;
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  padding: 0;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

/* 删除徽章过渡动画 */
.delete-badge-enter-active {
  transition: all 0.2s ease-out;
}
.delete-badge-leave-active {
  transition: all 0.15s ease-in;
}
.delete-badge-enter-from,
.delete-badge-leave-to {
  opacity: 0;
  transform: scale(0.5);
}

/* 编辑切换按钮 */
.edit-toggle-btn {
  background: var(--mobile-edit-cmd-bg);
  border: 1px solid var(--mobile-edit-cmd-border);
  color: var(--mobile-edit-cmd-color);
}

.edit-toggle-btn:active {
  transform: scale(0.95);
  background: var(--mobile-edit-cmd-active-bg);
}

/* 添加按钮 */
.add-cmd-btn {
  background: var(--mobile-add-cmd-bg);
  border: 1px dashed var(--mobile-add-cmd-border);
  color: var(--mobile-add-cmd-color);
}

.add-cmd-btn:active {
  transform: scale(0.95);
  background: var(--mobile-add-cmd-active-bg);
}

/* ==================== Add Dialog ==================== */

.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  padding: 1.5rem;
}

.dialog-box {
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 1rem;
  padding: 1.25rem;
  width: 100%;
  max-width: clamp(16rem, 20rem, 24rem);
}

.dialog-title {
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin-bottom: 1rem;
  text-align: center;
}

.dialog-input {
  width: 100%;
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 0.75rem;
  padding: 0.625rem 0.875rem;
  color: var(--mobile-text-primary);
  font-size: var(--font-size-base);
  outline: none;
  transition: border-color 0.2s ease;
  font-family: 'Courier New', monospace;
  box-sizing: border-box;
}

.dialog-input:focus {
  border-color: var(--mobile-accent);
}

.dialog-input::placeholder {
  color: var(--mobile-input-placeholder);
}

.dialog-actions {
  display: flex;
  gap: 0.75rem;
  margin-top: 1rem;
}

.dialog-btn {
  flex: 1;
  height: clamp(2rem, 2.25rem, 2.75rem);
  border-radius: 0.75rem;
  border: 1px solid;
  font-size: var(--font-size-base);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.dialog-btn.cancel {
  background: var(--mobile-bg-elevated);
  border-color: var(--mobile-border);
  color: var(--mobile-text-muted);
}

.dialog-btn.cancel:hover {
  background: var(--mobile-bg-secondary);
}

.dialog-btn.confirm {
  background: var(--mobile-confirm-bg);
  border-color: var(--mobile-confirm-border);
  color: var(--mobile-confirm-color);
}

.dialog-btn.confirm:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
