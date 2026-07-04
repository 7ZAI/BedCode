<template>
  <div
    class="input-bar bg-[var(--mobile-bg-secondary)] border-t border-[var(--mobile-border)] px-2 pt-2"
    :class="{ 'landscape-mode': isLandscapeMode }"
  >
    <!-- Special keys panel - 默认显示 -->
    <div v-if="showSpecialKeys && !isLandscapeMode" class="mb-2 grid grid-cols-8 gap-1.5">
      <button
        v-for="key in specialKeys"
        :key="key.code"
        class="bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-secondary)] text-xs py-2 rounded-lg active:bg-[var(--mobile-accent-muted)]"
        @click="sendSpecialKey(key.code)"
      >
        {{ key.label }}
      </button>
    </div>

    <!-- 横屏时显示的快捷键行 -->
    <div v-if="isLandscapeMode" class="mb-1.5 flex flex-wrap gap-1">
      <button
        v-for="key in specialKeys"
        :key="key.code"
        class="bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-secondary)] text-xs py-1 px-1.5 rounded active:bg-[var(--mobile-accent-muted)]"
        @click="sendSpecialKey(key.code)"
      >
        {{ key.label }}
      </button>
    </div>

    <!-- Main input row -->
    <div class="input-row">
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
          @blur="handleBlur"
          @input="adjustTextareaHeight"
          @keydown.enter.ctrl="submitText"
        ></textarea>
      </div>

      <!-- 发送按钮 -->
      <button
        class="action-btn send-btn"
        :disabled="!canSubmit"
        @click="submitText"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
        </svg>
      </button>

      <!-- 执行按钮 -->
      <button
        class="action-btn execute-btn"
        :disabled="!canSubmit"
        @click="executeText"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
      </button>

      <!-- Special keys toggle - 非横屏时显示 -->
      <button
        v-if="!isLandscapeMode"
        class="action-btn toggle-btn"
        :class="showSpecialKeys ? 'toggle-active' : 'toggle-inactive'"
        @click="toggleSpecialKeys"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
        </svg>
      </button>
    </div>

    <!-- Connection status -->
    <div v-if="showStatus" class="flex items-center justify-center gap-2 mt-1">
      <div
        :class="[
          'w-2 h-2 rounded-full',
          isConnected ? 'bg-[var(--mobile-success)]' : 'bg-[var(--mobile-error)]'
        ]"
      ></div>
      <span class="text-xs text-[var(--mobile-text-muted)]">
        {{ isConnected ? t('mobile.input.connected') : t('mobile.input.disconnected') }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  disabled?: boolean
  placeholder?: string
  isConnected?: boolean
  showStatus?: boolean
  isLandscape?: boolean
}>()

const emit = defineEmits<{
  submit: [text: string]
  execute: [text: string]
  specialKey: [key: string]
  focus: []
  blur: []
}>()

const inputText = ref('')
const inputRef = ref<HTMLTextAreaElement | null>(null)
const showSpecialKeys = ref(true)
const isLandscapeMode = computed(() => props.isLandscape || false)

const canSubmit = computed(() => {
  return inputText.value.trim().length > 0 && !props.disabled
})

function handleFocus() {
  emit('focus')
}

function handleBlur() {
  emit('blur')
}

function adjustTextareaHeight() {
  const textarea = inputRef.value
  if (!textarea) return

  textarea.style.height = 'auto'
  const newHeight = Math.min(textarea.scrollHeight, 120)
  textarea.style.height = `${newHeight}px`
}

const specialKeys = [
  { label: 'Tab', code: 'tab' },
  { label: 'Enter', code: 'enter' },
  { label: 'Esc', code: 'escape' },
  { label: 'Del', code: 'delete' },
  { label: 'Ctrl+C', code: 'ctrl_c' },
  { label: 'Ctrl+Z', code: 'ctrl_z' },
  { label: 'Ctrl+L', code: 'ctrl_l' },
  { label: '↑', code: 'arrow_up' },
  { label: '↓', code: 'arrow_down' },
  { label: '←', code: 'arrow_left' },
  { label: '→', code: 'arrow_right' },
]

function submitText() {
  if (inputText.value.trim()) {
    emit('submit', inputText.value)
    inputText.value = ''
    // 重置 textarea 高度
    if (inputRef.value) {
      inputRef.value.style.height = 'auto'
    }
  }
}

function executeText() {
  if (inputText.value.trim()) {
    emit('execute', inputText.value)
    inputText.value = ''
    // 重置 textarea 高度
    if (inputRef.value) {
      inputRef.value.style.height = 'auto'
    }
  }
}

function sendSpecialKey(code: string) {
  emit('specialKey', code)
}

function toggleSpecialKeys() {
  showSpecialKeys.value = !showSpecialKeys.value
}

function focus() {
  inputRef.value?.focus()
}

defineExpose({ focus })
</script>

<style scoped>
.input-bar {
  flex-shrink: 0;
}

.input-row {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  padding-bottom: 0.25rem;
}

.input-box {
  flex: 1;
  display: flex;
  align-items: flex-start;
  background: var(--mobile-input-bg, var(--mobile-bg-primary));
  border: 1px solid var(--mobile-input-border, var(--mobile-border));
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
  color: var(--mobile-input-placeholder, var(--mobile-text-muted));
}

.input-field:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.action-btn {
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
  background: var(--mobile-send-bg);
  border-color: var(--mobile-send-border);
  color: var(--mobile-send-color);
}

.send-btn:hover:not(:disabled) {
  background: var(--mobile-send-active-bg);
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.execute-btn {
  background: var(--mobile-execute-bg);
  border-color: var(--mobile-execute-border);
  color: var(--mobile-execute-color);
}

.execute-btn:hover:not(:disabled) {
  background: var(--mobile-execute-active-bg);
}

.execute-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.toggle-btn {
  background: var(--mobile-bg-elevated);
  border-color: var(--mobile-border);
  color: var(--mobile-text-muted);
}

.toggle-active {
  background: var(--mobile-add-cmd-bg);
  color: var(--mobile-add-cmd-color);
  border-color: var(--mobile-add-cmd-border);
}

.toggle-inactive {
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-muted);
  border-color: var(--mobile-border);
}
</style>
