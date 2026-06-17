<template>
  <div
    class="terminal-input-bar sticky left-0 right-0 bottom-0 z-40"
    :style="inputBarStyle"
  >
    <!-- 快捷键面板 - 覆盖层，不影响终端高度 -->
    <div v-if="showShortcutsPanel && !props.isLandscape" class="shortcuts-panel">
      <!-- 轮播容器 -->
      <div
        ref="carouselRef"
        class="carousel-container"
        @touchstart="onTouchStart"
        @touchmove="onTouchMove"
        @touchend="onTouchEnd"
      >
        <div class="carousel-track" :style="trackStyle">
          <!-- 第一页：快捷键 + 方向键 -->
          <div class="carousel-slide">
            <div class="shortcuts-layout">
              <!-- 左侧：一般快捷键 -->
              <div class="shortcuts-left">
                <div class="shortcuts-grid">
                  <button
                    v-for="key in generalShortcuts"
                    :key="key.code"
                    class="shortcut-btn"
                    @click="handleShortcutClick(key.code)"
                  >
                    {{ key.label }}
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

          <!-- 第二页：自定义命令 -->
          <div class="carousel-slide">
            <div class="custom-commands-layout">
              <div class="custom-commands-grid">
                <button
                  v-for="cmd in customCommands"
                  :key="cmd.id"
                  class="custom-cmd-btn"
                  :class="{ 'editing': isEditingCommands }"
                  @click="handleCustomCommandClick(cmd)"
                >
                  <span class="cmd-label">{{ cmd.command }}</span>
                  <transition name="delete-badge">
                    <button
                      v-if="isEditingCommands"
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
        </div>
      </div>

      <!-- 页码指示器 -->
      <div class="carousel-dots">
        <div
          class="dot"
          :class="{ active: currentSlide === 0 }"
          @click="goToSlide(0)"
        ></div>
        <div
          class="dot"
          :class="{ active: currentSlide === 1 }"
          @click="goToSlide(1)"
        ></div>
      </div>
    </div>

    <!-- 添加自定义命令弹窗 -->
    <Teleport to="body">
      <div v-if="showAddDialog" class="dialog-overlay" @click.self="showAddDialog = false">
        <div class="dialog-box">
          <div class="dialog-title">添加自定义命令</div>
          <input
            ref="cmdInputRef"
            v-model="newCommand"
            class="dialog-input"
            placeholder="输入命令，如 /clear"
            @keyup.enter="addCustomCommand"
          />
          <div class="dialog-actions">
            <button class="dialog-btn cancel" @click="showAddDialog = false">取消</button>
            <button class="dialog-btn confirm" :disabled="!newCommand.trim()" @click="addCustomCommand">确定</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- 输入区域 -->
    <div class="input-area">
      <!-- 快捷键切换按钮 -->
      <button
        class="toggle-btn"
        :class="showShortcutsPanel ? 'toggle-active' : 'toggle-inactive'"
        @click="toggleShortcuts"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
        </svg>
      </button>

      <!-- 输入框容器 -->
      <div class="input-box">
        <textarea
          ref="inputRef"
          v-model="inputText"
          class="input-field"
          :placeholder="placeholder"
          :disabled="disabled"
          rows="1"
          @focus="handleFocus"
          @input="adjustTextareaHeight"
        ></textarea>
      </div>

      <!-- 发送按钮 - 蓝色向上箭头 -->
      <button
        class="send-btn"
        :disabled="!canSubmit"
        @click="handleSubmit"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
        </svg>
      </button>

      <!-- 执行按钮 - Telegram 风格纸飞机 -->
      <button
        class="execute-btn"
        :disabled="!canSubmit"
        @click="handleExecute"
      >
        <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
          <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, inject, onMounted, nextTick, watch } from 'vue'
import type { Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// ==================== Types ====================

interface CustomCommand {
  id: string
  command: string
}

// ==================== Props ====================

const props = withDefaults(defineProps<{
  disabled?: boolean
  isConnected?: boolean
  showShortcuts?: boolean
  placeholder?: string
  isLandscape?: boolean
}>(), {
  disabled: false,
  isConnected: false,
  showShortcuts: false,
  placeholder: '输入命令...',
  isLandscape: false,
})

// ==================== Emits ====================

const emit = defineEmits<{
  submit: [text: string]
  execute: [text: string]
  specialKey: [key: string]
}>()

// ==================== Safe Area ====================

const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar: number }>>('safeArea')

const inputBarStyle = computed(() => {
  const jsBottom = safeArea?.value?.navigationBar || safeArea?.value?.bottom || 0
  return {
    paddingBottom: jsBottom > 0 ? `${jsBottom}px` : 'env(safe-area-inset-bottom, 0px)',
  }
})

// ==================== State ====================

const inputRef = ref<HTMLTextAreaElement | null>(null)
const inputText = ref('')
const showShortcutsPanel = ref(false)
const showAddDialog = ref(false)
const newCommand = ref('')
const cmdInputRef = ref<HTMLInputElement | null>(null)

// ==================== Carousel State ====================

const carouselRef = ref<HTMLElement | null>(null)
const currentSlide = ref(0)
const touchStartX = ref(0)
const touchStartY = ref(0)
const touchDeltaX = ref(0)
const isSwiping = ref(false)

const trackStyle = computed(() => ({
  transform: `translateX(${-currentSlide.value * 100 + touchDeltaX.value}%)`,
  transition: isSwiping.value ? 'none' : 'transform 0.3s ease',
}))

// ==================== Custom Commands ====================

const customCommands = ref<CustomCommand[]>([])
const isEditingCommands = ref(false)

// 从 Tauri settings 持久化加载自定义命令
async function loadCustomCommands() {
  try {
    const settings = await invoke<{ key: string; value: string }[]>('get_all_db_settings_mobile')
    const found = settings?.find(s => s.key === 'custom_commands')
    if (found?.value) {
      customCommands.value = JSON.parse(found.value)
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
  })
  saveCustomCommands()
  newCommand.value = ''
  showAddDialog.value = false
}

function handleCustomCommandClick(cmd: CustomCommand) {
  // 编辑模式下点击不执行命令
  if (isEditingCommands.value) return
  emit('execute', cmd.command)
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

const generalShortcuts = [
  { label: 'Tab', code: 'tab' },
  { label: 'Enter', code: 'enter' },
  { label: 'Esc', code: 'escape' },
  { label: 'Del', code: 'backspace' },
  { label: 'Ctrl+C', code: 'ctrl_c' },
  { label: 'Ctrl+Z', code: 'ctrl_z' },
  { label: 'Ctrl+L', code: 'ctrl_l' },
]

// ==================== Carousel Methods ====================

function goToSlide(index: number) {
  currentSlide.value = Math.max(0, Math.min(index, 1))
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
    // 将像素偏移转为百分比
    touchDeltaX.value = (deltaX / width) * 100
  }
}

function onTouchEnd() {
  isSwiping.value = false
  const threshold = 20 // 滑动超过 20% 切换页面

  if (touchDeltaX.value < -threshold && currentSlide.value < 1) {
    currentSlide.value = 1
  } else if (touchDeltaX.value > threshold && currentSlide.value > 0) {
    currentSlide.value = 0
  }

  touchDeltaX.value = 0
}

// ==================== Computed ====================

const canSubmit = computed(() => {
  return inputText.value.trim().length > 0 && !props.disabled
})

// ==================== Methods ====================

function toggleShortcuts() {
  showShortcutsPanel.value = !showShortcutsPanel.value
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
  emit('specialKey', code)
}

function handleFocus() {
  setTimeout(() => {
    if (inputRef.value) {
      inputRef.value.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, 100)
}

function adjustTextareaHeight() {
  const textarea = inputRef.value
  if (!textarea) return
  textarea.style.height = 'auto'
  const newHeight = Math.min(textarea.scrollHeight, 120)
  textarea.style.height = `${newHeight}px`
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
}

.input-area {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.toggle-btn {
  width: 2rem;
  height: 2rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  border: 1px solid;
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
}

.toggle-active {
  background: rgba(139, 233, 253, 0.15);
  color: #8be9fd;
  border-color: rgba(139, 233, 253, 0.5);
}

.toggle-inactive {
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-muted);
  border-color: var(--mobile-border);
}

.input-box {
  flex: 1;
  display: flex;
  align-items: flex-start;
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 1rem;
  padding: 0.5rem 1rem;
  transition: border-color 0.2s ease;
  min-height: 2.5rem;
}

.input-box:focus-within {
  border-color: var(--mobile-accent);
}

.input-field {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  color: var(--mobile-text-primary);
  font-size: 0.875rem;
  font-family: inherit;
  resize: none;
  max-height: 120px;
  overflow-y: auto;
  line-height: 1.5;
}

.input-field::placeholder {
  color: var(--mobile-input-placeholder);
}

.send-btn,
.execute-btn {
  width: 2.5rem;
  height: 2.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  border: 1px solid;
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
}

.send-btn {
  background: linear-gradient(135deg, rgba(59, 130, 246, 0.15), rgba(59, 130, 246, 0.08));
  border-color: rgba(59, 130, 246, 0.4);
  color: #3b82f6;
}

.send-btn:hover:not(:disabled) {
  background: linear-gradient(135deg, rgba(59, 130, 246, 0.25), rgba(59, 130, 246, 0.15));
  border-color: rgba(59, 130, 246, 0.6);
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.execute-btn {
  background: linear-gradient(135deg, rgba(255, 184, 108, 0.2), rgba(255, 121, 198, 0.15));
  border-color: rgba(255, 184, 108, 0.5);
  color: #ffb86c;
}

.execute-btn:hover:not(:disabled) {
  background: linear-gradient(135deg, rgba(255, 184, 108, 0.35), rgba(255, 121, 198, 0.25));
  border-color: rgba(255, 184, 108, 0.7);
}

.execute-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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
  box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.2);
}

.carousel-container {
  overflow: hidden;
}

.carousel-track {
  display: flex;
  will-change: transform;
}

.carousel-slide {
  min-width: 100%;
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

.shortcuts-layout {
  display: flex;
  gap: 0.75rem;
}

.shortcuts-left {
  flex: 1;
  min-width: 0;
}

.shortcuts-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.375rem;
}

.shortcut-btn {
  height: 2.25rem;
  background: linear-gradient(135deg, rgba(189, 147, 249, 0.12), rgba(189, 147, 249, 0.06));
  border: 1px solid rgba(189, 147, 249, 0.35);
  color: #bd93f9;
  font-size: 0.75rem;
  font-weight: 500;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.shortcut-btn:hover {
  background: linear-gradient(135deg, rgba(189, 147, 249, 0.22), rgba(189, 147, 249, 0.12));
  border-color: rgba(189, 147, 249, 0.55);
}

.shortcut-btn:active {
  transform: scale(0.95);
  background: linear-gradient(135deg, rgba(189, 147, 249, 0.28), rgba(189, 147, 249, 0.16));
}

.shortcuts-right {
  flex-shrink: 0;
  width: auto;
}

.arrow-keys-layout {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.arrow-row {
  display: flex;
  gap: 0.25rem;
  justify-content: center;
}

.arrow-placeholder {
  width: 2.25rem;
  height: 2.25rem;
}

.arrow-btn {
  width: 2.25rem;
  height: 2.25rem;
  background: linear-gradient(135deg, rgba(241, 250, 140, 0.12), rgba(241, 250, 140, 0.06));
  border: 1px solid rgba(241, 250, 140, 0.35);
  color: #f1fa8c;
  border-radius: 0.5rem;
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.arrow-btn:hover {
  background: linear-gradient(135deg, rgba(241, 250, 140, 0.22), rgba(241, 250, 140, 0.12));
  border-color: rgba(241, 250, 140, 0.55);
}

.arrow-btn:active {
  transform: scale(0.9);
  background: linear-gradient(135deg, rgba(241, 250, 140, 0.28), rgba(241, 250, 140, 0.16));
}

.arrow-icon {
  width: 1rem;
  height: 1rem;
}

/* ==================== Custom Commands (Slide 2) ==================== */

.custom-commands-layout {
  min-height: 5rem;
}

.custom-commands-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.375rem;
}

.custom-cmd-btn {
  height: 2.25rem;
  background: linear-gradient(135deg, rgba(80, 250, 123, 0.12), rgba(80, 250, 123, 0.06));
  border: 1px solid rgba(80, 250, 123, 0.35);
  color: #50fa7b;
  font-size: 0.75rem;
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

.custom-cmd-btn:hover {
  background: linear-gradient(135deg, rgba(80, 250, 123, 0.22), rgba(80, 250, 123, 0.12));
  border-color: rgba(80, 250, 123, 0.55);
}

.custom-cmd-btn:active {
  transform: scale(0.95);
  background: linear-gradient(135deg, rgba(80, 250, 123, 0.28), rgba(80, 250, 123, 0.16));
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
  border-color: rgba(255, 85, 85, 0.5);
  background: linear-gradient(135deg, rgba(255, 85, 85, 0.12), rgba(255, 85, 85, 0.06));
  color: #ff5555;
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
  background: rgba(255, 85, 85, 0.9);
  border: none;
  border-radius: 9999px;
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  padding: 0;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
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
  background: linear-gradient(135deg, rgba(255, 184, 108, 0.12), rgba(255, 184, 108, 0.06));
  border: 1px solid rgba(255, 184, 108, 0.35);
  color: #ffb86c;
}

.edit-toggle-btn:hover {
  background: linear-gradient(135deg, rgba(255, 184, 108, 0.22), rgba(255, 184, 108, 0.12));
  border-color: rgba(255, 184, 108, 0.55);
}

.edit-toggle-btn:active {
  transform: scale(0.95);
  background: linear-gradient(135deg, rgba(255, 184, 108, 0.28), rgba(255, 184, 108, 0.16));
}

/* 添加按钮 */
.add-cmd-btn {
  background: linear-gradient(135deg, rgba(139, 233, 253, 0.12), rgba(139, 233, 253, 0.06));
  border: 1px dashed rgba(139, 233, 253, 0.5);
  color: #8be9fd;
}

.add-cmd-btn:hover {
  background: linear-gradient(135deg, rgba(139, 233, 253, 0.22), rgba(139, 233, 253, 0.12));
  border-color: rgba(139, 233, 253, 0.7);
}

.add-cmd-btn:active {
  transform: scale(0.95);
  background: linear-gradient(135deg, rgba(139, 233, 253, 0.28), rgba(139, 233, 253, 0.16));
}

/* ==================== Add Dialog ==================== */

.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
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
  max-width: 20rem;
}

.dialog-title {
  font-size: 1rem;
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
  font-size: 0.875rem;
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
  height: 2.25rem;
  border-radius: 0.75rem;
  border: 1px solid;
  font-size: 0.875rem;
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
  background: linear-gradient(135deg, rgba(80, 250, 123, 0.2), rgba(80, 250, 123, 0.1));
  border-color: rgba(80, 250, 123, 0.5);
  color: #50fa7b;
}

.dialog-btn.confirm:hover {
  background: linear-gradient(135deg, rgba(80, 250, 123, 0.3), rgba(80, 250, 123, 0.15));
  border-color: rgba(80, 250, 123, 0.7);
}

.dialog-btn.confirm:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
