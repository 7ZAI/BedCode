<template>
  <div
    class="terminal-input-bar fixed left-0 right-0 z-40 bg-white dark:bg-dark-800 border-t border-gray-200 dark:border-dark-700"
    :style="containerStyle"
  >
    <!-- 快捷键面板 - 横屏时默认折叠 -->
    <div v-if="showShortcutsPanel && !props.isLandscape" class="shortcuts-panel px-2 pt-2">
      <div class="grid grid-cols-4 gap-1.5">
        <button
          v-for="key in shortcuts"
          :key="key.code"
          class="shortcut-btn h-8 bg-gray-100 dark:bg-dark-700 text-gray-600 dark:text-dark-300 text-xs rounded-lg active:bg-gray-200 dark:active:bg-dark-600"
          @click="handleShortcutClick(key.code)"
        >
          {{ key.label }}
        </button>
      </div>
    </div>

    <!-- 输入区域 -->
    <div class="input-area flex items-end gap-2 px-2 py-2">
      <!-- 快捷键切换按钮 -->
      <button
        class="toggle-btn shrink-0 w-8 h-8 flex items-center justify-center rounded-lg"
        :class="showShortcutsPanel ? 'bg-primary-100 dark:bg-primary-900/30 text-primary-600' : 'bg-gray-100 dark:bg-dark-700 text-gray-500 dark:text-dark-400'"
        @click="toggleShortcuts"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
        </svg>
      </button>

      <!-- 输入框 -->
      <textarea
        ref="inputRef"
        v-model="inputText"
        class="flex-1 min-h-[40px] max-h-[120px] bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-gray-900 dark:text-dark-100 placeholder-gray-400 dark:placeholder-dark-400 resize-none focus:outline-none focus:border-primary-500"
        :class="{ 'min-h-[32px] max-h-[60px]': props.isLandscape }"
        :placeholder="placeholder"
        :disabled="disabled"
        rows="1"
        @input="autoResize"
        @keydown.enter.ctrl="handleExecute"
      ></textarea>

      <!-- 发送按钮 -->
      <button
        class="send-btn shrink-0 h-10 px-3 bg-gray-200 dark:bg-dark-600 text-gray-700 dark:text-dark-200 text-sm rounded-lg disabled:opacity-50"
        :disabled="!canSubmit"
        @click="handleSubmit"
      >
        发送
      </button>

      <!-- 执行按钮 -->
      <button
        class="execute-btn shrink-0 h-10 px-3 bg-primary-600 text-white text-sm rounded-lg disabled:opacity-50"
        :disabled="!canSubmit"
        @click="handleExecute"
      >
        执行
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'

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
  showShortcuts: true,
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

const inputRef = ref<HTMLTextAreaElement | null>(null)
const inputText = ref('')
const showShortcutsPanel = ref(props.showShortcuts)
const keyboardHeight = ref(0)

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

const containerStyle = computed(() => ({
  // 组件固定在底部，使用 transform 上移来避开键盘
  transform: `translateY(-${keyboardHeight.value}px) translateZ(0)`,
  transition: 'transform 0.1s ease-out',
}))

// ==================== Methods ====================

function toggleShortcuts() {
  showShortcutsPanel.value = !showShortcutsPanel.value
}

function autoResize() {
  const el = inputRef.value
  if (!el) return
  el.style.height = 'auto'
  const maxHeight = props.isLandscape ? 60 : 120
  el.style.height = Math.min(el.scrollHeight, maxHeight) + 'px'
}

function handleSubmit() {
  const text = inputText.value.trim()
  if (!text) return
  emit('submit', text)
  inputText.value = ''
  autoResize()
}

function handleExecute() {
  const text = inputText.value.trim()
  if (!text) return
  emit('execute', text)
  inputText.value = ''
  autoResize()
}

function handleShortcutClick(code: string) {
  emit('specialKey', code)
}

// ==================== Keyboard Avoidance ====================

function setupVisualViewport() {
  const vv = window.visualViewport
  if (!vv) {
    console.warn('[TerminalInputBar] visualViewport not supported, using fallback')
    // Fallback: 监听 window resize 事件
    const handleResize = () => {
      // Android WebView 可能在键盘弹出时改变 document 高度而非 window.innerHeight
      const docHeight = document.documentElement.clientHeight
      const windowHeight = window.innerHeight
      // 使用 document 高度差作为键盘高度的近似值
      const estimatedKeyboardHeight = Math.max(0, windowHeight - docHeight)
      console.log('[TerminalInputBar] Fallback: windowHeight=', windowHeight, 'docHeight=', docHeight, 'keyboard=', estimatedKeyboardHeight)
      keyboardHeight.value = estimatedKeyboardHeight
    }
    window.addEventListener('resize', handleResize)
    return () => {
      window.removeEventListener('resize', handleResize)
    }
  }

  console.log('[TerminalInputBar] visualViewport supported')

  const handleResize = () => {
    const height = Math.max(0, window.innerHeight - vv!.height)
    console.log('[TerminalInputBar] visualViewport resize: window.innerHeight=', window.innerHeight, 'vv.height=', vv!.height, 'keyboard=', height)
    keyboardHeight.value = height
  }

  vv.addEventListener('resize', handleResize)
  vv.addEventListener('scroll', handleResize)

  // 初始调用一次，获取当前状态
  handleResize()

  // 返回清理函数
  return () => {
    vv.removeEventListener('resize', handleResize)
    vv.removeEventListener('scroll', handleResize)
  }
}

let cleanupViewport: (() => void) | undefined

onMounted(() => {
  cleanupViewport = setupVisualViewport()
})

onUnmounted(() => {
  cleanupViewport?.()
})
</script>

<style scoped>
.terminal-input-bar {
  box-shadow: 0 -2px 8px rgba(0, 0, 0, 0.05);
  /* 使用 padding-bottom 处理底部安全区域，避免被系统导航遮挡 */
  padding-bottom: env(safe-area-inset-bottom, 0px);
}

.shortcuts-panel {
  border-bottom: 1px solid theme('colors.gray.100');
}

.dark .shortcuts-panel {
  border-bottom-color: theme('colors.dark.700');
}

.shortcut-btn {
  transition: background-color 0.15s ease;
}

.shortcut-btn:active {
  transform: scale(0.95);
}
</style>