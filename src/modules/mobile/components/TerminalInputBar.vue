<template>
  <div
    class="terminal-input-bar sticky left-0 right-0 bottom-0 z-40 bg-[#12121a]/95 backdrop-blur-xl border-t border-cyan-500/20"
  >
    <!-- 快捷键面板 - 点击按钮后显示 -->
    <div v-if="showShortcutsPanel && !props.isLandscape" class="shortcuts-panel px-2 pt-2">
      <div class="grid grid-cols-4 gap-1.5">
        <button
          v-for="key in shortcuts"
          :key="key.code"
          class="shortcut-btn h-8 bg-cyan-500/10 border border-cyan-500/20 text-cyan-400 text-xs rounded-lg hover:bg-cyan-500/20 transition-colors"
          @click="handleShortcutClick(key.code)"
        >
          {{ key.label }}
        </button>
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
        <input
          ref="inputRef"
          v-model="inputText"
          type="text"
          class="input-field"
          :placeholder="placeholder"
          :disabled="disabled"
          @focus="handleFocus"
        />
      </div>

      <!-- 发送按钮 -->
      <button
        class="send-btn"
        :disabled="!canSubmit"
        @click="handleSubmit"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
        </svg>
      </button>

      <!-- 执行按钮 -->
      <button
        class="execute-btn"
        :disabled="!canSubmit"
        @click="handleExecute"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

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

// ==================== State ====================

const inputRef = ref<HTMLInputElement | null>(null)
const inputText = ref('')
const showShortcutsPanel = ref(false)  // 默认隐藏

// ==================== Shortcuts Data ====================

const shortcuts = [
  { label: 'Tab', code: 'tab' },
  { label: 'Enter', code: 'enter' },
  { label: 'Esc', code: 'escape' },
  { label: 'Del', code: 'backspace' },
  { label: 'Ctrl+C', code: 'ctrl_c' },
  { label: 'Ctrl+Z', code: 'ctrl_z' },
  { label: 'Ctrl+L', code: 'ctrl_l' },
  { label: '↑', code: 'arrow_up' },
  { label: '↓', code: 'arrow_down' },
  { label: '←', code: 'arrow_left' },
  { label: '→', code: 'arrow_right' },
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
}

function handleExecute() {
  const text = inputText.value.trim()
  if (!text) return
  emit('execute', text)
  inputText.value = ''
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
</script>

<style scoped>
.terminal-input-bar {
  flex-shrink: 0;
  background: rgba(18, 18, 26, 0.95);
  backdrop-filter: blur(20px);
  border-top: 1px solid rgba(0, 212, 255, 0.15);
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
  background: rgba(0, 212, 255, 0.2);
  color: #00d4ff;
  border-color: rgba(0, 212, 255, 0.3);
}

.toggle-inactive {
  background: #1f2937;
  color: #6b7280;
  border-color: #374151;
}

.input-box {
  flex: 1;
  display: flex;
  align-items: center;
  background: #0a0a0f;
  border: 1px solid rgba(0, 212, 255, 0.2);
  border-radius: 9999px;
  padding: 0.5rem 1rem;
  transition: border-color 0.2s ease;
}

.input-box:focus-within {
  border-color: rgba(0, 212, 255, 0.5);
}

.input-field {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  color: #ffffff;
  font-size: 0.875rem;
  font-family: inherit;
}

.input-field::placeholder {
  color: #4b5563;
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
  background: #1f2937;
  border-color: #374151;
  color: #9ca3af;
}

.send-btn:hover:not(:disabled) {
  border-color: rgba(0, 212, 255, 0.3);
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.execute-btn {
  background: rgba(0, 212, 255, 0.2);
  border-color: rgba(0, 212, 255, 0.3);
  color: #00d4ff;
}

.execute-btn:hover:not(:disabled) {
  background: rgba(0, 212, 255, 0.3);
}

.execute-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.shortcuts-panel {
  border-bottom: 1px solid rgba(0, 212, 255, 0.1);
}

.shortcut-btn {
  transition: background-color 0.15s ease;
}

.shortcut-btn:active {
  transform: scale(0.95);
}
</style>
