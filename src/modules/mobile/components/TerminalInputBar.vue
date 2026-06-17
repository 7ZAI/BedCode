<template>
  <div
    class="terminal-input-bar sticky left-0 right-0 bottom-0 z-40"
    :style="inputBarStyle"
  >
    <!-- 快捷键面板 - 点击按钮后显示 -->
    <div v-if="showShortcutsPanel && !props.isLandscape" class="shortcuts-panel">
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
            <!-- 第一行：上箭头居中 -->
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
            <!-- 第二行：左、下、右 -->
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
import { ref, computed, inject } from 'vue'
import type { Ref } from 'vue'

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
  showShortcuts: false,  // 默认隐藏快捷键面板
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

// 输入栏样式：底部安全区由 paddingBottom 承担
// 使用 max() 确保 JS 值和 CSS env() 取较大值
// JS 初始化延迟时 CSS env() 也能立即生效
const inputBarStyle = computed(() => {
  const jsBottom = safeArea?.value?.navigationBar || safeArea?.value?.bottom || 0
  return {
    paddingBottom: jsBottom > 0 ? `${jsBottom}px` : 'env(safe-area-inset-bottom, 0px)',
  }
})

// ==================== State ====================

const inputRef = ref<HTMLTextAreaElement | null>(null)
const inputText = ref('')
const showShortcutsPanel = ref(false)  // 默认隐藏

// ==================== Shortcuts Data ====================

// 一般快捷键（不含方向键）
const generalShortcuts = [
  { label: 'Tab', code: 'tab' },
  { label: 'Enter', code: 'enter' },
  { label: 'Esc', code: 'escape' },
  { label: 'Del', code: 'backspace' },
  { label: 'Ctrl+C', code: 'ctrl_c' },
  { label: 'Ctrl+Z', code: 'ctrl_z' },
  { label: 'Ctrl+L', code: 'ctrl_l' },
]

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
  // 重置 textarea 高度
  if (inputRef.value) {
    inputRef.value.style.height = 'auto'
  }
}

function handleExecute() {
  const text = inputText.value.trim()
  if (!text) return
  emit('execute', text)
  inputText.value = ''
  // 重置 textarea 高度
  if (inputRef.value) {
    inputRef.value.style.height = 'auto'
  }
}

function handleShortcutClick(code: string) {
  emit('specialKey', code)
}

// 处理输入框获得焦点
function handleFocus() {
  // 延迟执行，等待键盘弹出后再滚动
  setTimeout(() => {
    if (inputRef.value) {
      inputRef.value.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, 100)
}

// 自动调整 textarea 高度
function adjustTextareaHeight() {
  const textarea = inputRef.value
  if (!textarea) return

  // 先重置高度以获取正确的 scrollHeight
  textarea.style.height = 'auto'
  // 设置为新高度，但不超过最大高度
  const newHeight = Math.min(textarea.scrollHeight, 120)
  textarea.style.height = `${newHeight}px`
}
</script>

<style scoped>
.terminal-input-bar {
  flex-shrink: 0;
  background: var(--mobile-bg-secondary);
  backdrop-filter: blur(20px);
  border-top: 1px solid var(--mobile-border);
  padding: 0.5rem 1rem;
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

.shortcuts-panel {
  border-bottom: 1px solid var(--mobile-border);
  padding: 0.5rem 0.75rem;
}

.shortcuts-layout {
  display: flex;
  gap: 0.75rem;
}

/* 左侧：一般快捷键 */
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

/* 右侧：方向键布局 */
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
</style>
