<template>
  <div
    class="terminal-input-bar fixed left-0 right-0 bottom-0 z-40 bg-white dark:bg-dark-800 border-t border-gray-200 dark:border-dark-700"
    :style="containerStyle"
  >
    <!-- 快捷键面板 - 点击按钮后显示 -->
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

    <!-- 输入区域 - AI 对话框样式 -->
    <div class="input-area flex items-center gap-2 px-3 py-2">
      <!-- 快捷键切换按钮 -->
      <button
        class="toggle-btn shrink-0 w-8 h-8 flex items-center justify-center rounded-full"
        :class="showShortcutsPanel ? 'bg-primary-100 dark:bg-primary-900/30 text-primary-600' : 'bg-gray-100 dark:bg-dark-700 text-gray-500 dark:text-dark-400'"
        @click="toggleShortcuts"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
        </svg>
      </button>

      <!-- 输入框容器 - 圆角胶囊样式 -->
      <div class="flex-1 flex items-center bg-gray-100 dark:bg-dark-700 rounded-full px-4 py-2">
        <input
          ref="inputRef"
          v-model="inputText"
          type="text"
          class="flex-1 bg-transparent text-sm text-gray-900 dark:text-dark-100 placeholder-gray-400 dark:placeholder-dark-400 focus:outline-none"
          :placeholder="placeholder"
          :disabled="disabled"
          @focus="handleFocus"
        />
      </div>

      <!-- 发送按钮 -->
      <button
        class="send-btn shrink-0 w-10 h-10 flex items-center justify-center bg-gray-200 dark:bg-dark-600 text-gray-700 dark:text-dark-200 rounded-full disabled:opacity-50"
        :disabled="!canSubmit"
        @click="handleSubmit"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
        </svg>
      </button>

      <!-- 执行按钮 -->
      <button
        class="execute-btn shrink-0 w-10 h-10 flex items-center justify-center bg-primary-600 text-white rounded-full disabled:opacity-50"
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

// 计算容器样式
// 1. 底部安全区域：通过 CSS env(safe-area-inset-bottom) 自动获取
// 2. 键盘弹出时：通过 translateY 向上移动键盘高度
const containerStyle = computed(() => {
  const styles: Record<string, string> = {
    transition: 'transform 0.15s ease-out',
  }

  // 键盘弹出时向上移动
  if (keyboardHeight.value > 0) {
    styles.transform = `translateY(-${keyboardHeight.value}px) translateZ(0)`
  } else {
    styles.transform = 'translateY(0) translateZ(0)'
  }

  return styles
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
// 在 adjustNothing 模式下，需要手动确保输入框可见
function handleFocus() {
  // 延迟执行，等待键盘弹出后再滚动
  setTimeout(() => {
    if (inputRef.value) {
      inputRef.value.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, 100)
}

// ==================== Keyboard Avoidance ====================

function setupKeyboardListener() {
  console.log('[TerminalInputBar] setupKeyboardListener: starting setup')

  // 使用 Visual Viewport API（adjustNothing 模式下最可靠）
  // 布局视口 - 可视视口 = 键盘高度
  const vv = window.visualViewport
  if (vv) {
    console.log('[TerminalInputBar] Using visualViewport API (adjustNothing mode)')

    const handleViewportChange = () => {
      // 在 adjustNothing 模式下：
      // window.innerHeight = 布局视口高度（不变）
      // visualViewport.height = 实际可见区域高度（键盘弹出时变小）
      const windowHeight = window.innerHeight
      const viewportHeight = vv!.height
      const viewportTop = vv!.offsetTop  // 可视视口顶部距离布局视口顶部的距离
      const height = Math.max(0, windowHeight - viewportHeight - viewportTop)

      console.log('[TerminalInputBar] visualViewport: layoutHeight=', windowHeight, 'viewportHeight=', viewportHeight, 'viewportTop=', viewportTop, 'keyboardHeight=', height)

      // 防止抖动：只有高度变化超过 2px 才更新
      if (Math.abs(height - keyboardHeight.value) > 2 || (height === 0 && keyboardHeight.value > 0)) {
        keyboardHeight.value = height
      }
    }

    vv.addEventListener('resize', handleViewportChange)
    vv.addEventListener('scroll', handleViewportChange)

    // 初始检查
    handleViewportChange()

    return () => {
      console.log('[TerminalInputBar] Cleaning up visualViewport listener')
      vv.removeEventListener('resize', handleViewportChange)
      vv.removeEventListener('scroll', handleViewportChange)
    }
  }

  // visualViewport 不可用的 fallback（极少数情况）
  console.log('[TerminalInputBar] visualViewport not available, keyboard avoidance disabled')
  return () => {}
}

let cleanupKeyboard: (() => void) | undefined

onMounted(() => {
  cleanupKeyboard = setupKeyboardListener()
})

onUnmounted(() => {
  cleanupKeyboard?.()
})
</script>

<style scoped>
.terminal-input-bar {
  box-shadow: 0 -2px 8px rgba(0, 0, 0, 0.05);
  /* 底部安全区域：避免被系统导航栏遮挡 */
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