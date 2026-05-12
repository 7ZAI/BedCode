<template>
  <div
    class="fixed z-[999]"
    :style="floatingButtonStyle"
    @touchstart="onTouchStart"
    @touchmove="onTouchMove"
    @touchend="onTouchEnd"
  >
    <!-- 悬浮球按钮 -->
    <button
      class="w-12 h-12 rounded-full flex items-center justify-center shadow-lg transition-transform"
      :class="{ 'scale-95': isDragging }"
      :style="{ background: 'linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%)' }"
      @click="handleMainButtonClick"
    >
      <svg class="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
      </svg>
    </button>

    <!-- 扇形展开的功能按钮 -->
    <transition name="expand">
      <div v-if="store.isExpanded" class="absolute inset-0">
        <!-- 中心按钮作为触发的绝对定位参考 -->
        <!-- 清屏 - 上 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          :style="{
            top: '50%',
            left: '50%',
            transform: 'translate(-50%, -60px)',
            transition: 'transform 0.3s ease'
          }"
          @click.stop="handleClear"
        >
          <svg class="w-5 h-5 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>

        <!-- 输入 - 左上 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          :style="{
            top: '50%',
            left: '50%',
            transform: 'translate(calc(-50% - 42px), calc(-50% - 42px))',
            transition: 'transform 0.3s ease'
          }"
          @click.stop="handleInput"
        >
          <svg class="w-5 h-5 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </button>

        <!-- Ctrl+C - 右上 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          :style="{
            top: '50%',
            left: '50%',
            transform: 'translate(calc(-50% + 42px), calc(-50% - 42px))',
            transition: 'transform 0.3s ease'
          }"
          @click.stop="handleCtrlC"
        >
          <svg class="w-5 h-5 text-amber-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
        </button>

        <!-- 快捷键 - 下 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          :style="{
            top: '50%',
            left: '50%',
            transform: 'translate(-50%, 60px)',
            transition: 'transform 0.3s ease'
          }"
          @click.stop="handleShortcut"
        >
          <svg class="w-5 h-5 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
        </button>
      </div>
    </transition>
  </div>

  <!-- 快捷键面板 -->
  <ShortcutPanel
    :visible="showShortcutPanel"
    @close="showShortcutPanel = false"
    @select="handleShortcutSelect"
  />

  <!-- 输入弹窗 -->
  <Teleport to="body">
    <div
      v-if="showInputModal"
      class="fixed inset-0 z-[100] flex items-center justify-center p-4"
      @click.self="showInputModal = false"
    >
      <div class="absolute inset-0 bg-black/50" @click="showInputModal = false"></div>
      <div class="relative bg-white dark:bg-dark-800 rounded-xl w-full max-w-md p-4 shadow-xl">
        <div class="text-sm font-medium text-gray-700 dark:text-dark-200 mb-3">
          输入命令
        </div>
        <textarea
          ref="modalInputRef"
          v-model="inputText"
          class="w-full bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-gray-900 dark:text-dark-100 placeholder-dark-400 focus:outline-none focus:border-primary-500 resize-none"
          placeholder="输入命令..."
          rows="4"
        ></textarea>
        <div class="flex justify-between gap-2 mt-4">
          <button
            class="px-4 py-2 text-sm text-gray-600 dark:text-dark-300"
            @click="showInputModal = false"
          >
            取消
          </button>
          <div class="flex gap-2">
            <button
              class="px-4 py-2 text-sm bg-gray-200 dark:bg-dark-600 text-gray-700 dark:text-dark-200 rounded-lg"
              :disabled="!inputText.trim()"
              @click="submitText"
            >
              发送
            </button>
            <button
              class="px-4 py-2 text-sm bg-primary-600 text-white rounded-lg"
              :disabled="!inputText.trim()"
              @click="executeText"
            >
              执行
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from 'vue'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import ShortcutPanel from './ShortcutPanel.vue'

const props = defineProps<{
  terminalRef: any
  terminalInstance: any
  isConnected: boolean
}>()

const store = useInputAssistantStore()

// UI 状态
const showShortcutPanel = ref(false)
const showInputModal = ref(false)
const inputText = ref('')
const modalInputRef = ref<HTMLTextAreaElement | null>(null)

// 拖动状态
const isDragging = ref(false)
const dragStartPos = ref({ x: 0, y: 0 })
const buttonRect = ref({ x: 0, y: 0 })

// 屏幕尺寸（用于限制拖动范围）
const screenSize = ref({ width: window.innerWidth, height: window.innerHeight })

// 悬浮球位置样式
const floatingButtonStyle = computed(() => {
  const pos = store.position
  const edge = 16 // 距屏幕边缘的距离
  const size = 48 // 按钮直径

  // 如果位置为默认值(-1,-1)，使用右侧居中默认位置
  if (pos.x < 0 || pos.y < 0) {
    return {
      right: `${edge}px`,
      top: '50%',
      transform: 'translateY(-50%)',
    }
  }

  // 限制在屏幕范围内
  const x = Math.max(edge, Math.min(pos.x, screenSize.value.width - size - edge))
  const y = Math.max(edge + 60, Math.min(pos.y, screenSize.value.height - size - edge))

  return {
    right: `${screenSize.value.width - x - size}px`,
    top: `${y}px`,
  }
})

// 监听弹窗打开，聚焦输入框
watch(showInputModal, async (show) => {
  if (show) {
    inputText.value = ''
    await nextTick()
    modalInputRef.value?.focus()
  }
})

// 拖动处理
function onTouchStart(e: TouchEvent) {
  if (store.isExpanded) {
    // 如果菜单展开，先收起
    store.collapse()
    return
  }

  isDragging.value = true
  const touch = e.touches[0]
  dragStartPos.value = { x: touch.clientX, y: touch.clientY }

  const btn = (e.currentTarget as HTMLElement).getBoundingClientRect()
  buttonRect.value = { x: btn.left, y: btn.top }
}

function onTouchMove(e: TouchEvent) {
  if (!isDragging.value) return

  e.preventDefault()
  const touch = e.touches[0]

  // 计算新位置（相对于按钮中心）
  const deltaX = touch.clientX - dragStartPos.value.x
  const deltaY = touch.clientY - dragStartPos.value.y

  const newX = buttonRect.value.x + deltaX + 24 // +24 为半径
  const newY = buttonRect.value.y + deltaY + 24

  store.savePosition(newX, newY)
}

function onTouchEnd() {
  isDragging.value = false
}

// 点击悬浮球主按钮
function handleMainButtonClick() {
  if (store.isExpanded) {
    store.collapse()
  } else {
    store.toggleExpanded()
  }
}

// 输入
function handleInput() {
  store.collapse()
  showInputModal.value = true
}

function submitText() {
  if (inputText.value.trim()) {
    props.terminalInstance.sendInput(inputText.value)
    inputText.value = ''
    showInputModal.value = false
  }
}

function executeText() {
  if (inputText.value.trim()) {
    props.terminalInstance.sendInput(inputText.value)
    setTimeout(() => {
      props.terminalInstance.sendSpecialKey('enter')
    }, 50)
    inputText.value = ''
    showInputModal.value = false
  }
}

// 快捷键
function handleShortcut() {
  store.collapse()
  showShortcutPanel.value = true
}

function handleShortcutSelect(key: string) {
  if (props.isConnected) {
    props.terminalInstance.sendSpecialKey(key)
  }
}

// 清屏
function handleClear() {
  store.collapse()
  props.terminalRef.value?.clear()
}

// Ctrl+C
function handleCtrlC() {
  store.collapse()
  if (props.isConnected) {
    props.terminalInstance.sendSpecialKey('ctrl_c')
  }
}

// 监听窗口大小变化
onMounted(() => {
  window.addEventListener('resize', () => {
    screenSize.value = { width: window.innerWidth, height: window.innerHeight }
  })
})
</script>

<style scoped>
.expand-enter-active,
.expand-leave-active {
  transition: all 0.3s ease;
}

.expand-enter-from,
.expand-leave-to {
  opacity: 0;
}

.expand-enter-from button,
.expand-leave-to button {
  transform: translate(-50%, -50%) scale(0) !important;
}
</style>
