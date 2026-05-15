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
      class="rounded-full flex items-center justify-center shadow-lg transition-transform"
      :class="{ 'scale-95': isDragging }"
      :style="{
        background: 'linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%)',
        width: `${store.settings.size}px`,
        height: `${store.settings.size}px`,
        padding: `${Math.max(8, store.settings.size * 0.2)}px`
      }"
      :aria-label="store.isExpanded ? '关闭菜单' : '打开菜单'"
      @click="handleMainButtonClick"
    >
      <svg class="text-white" :class="iconSizeClass" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
      </svg>
    </button>

    <!-- 环形展开的功能按钮 -->
    <transition name="expand">
      <div v-if="store.isExpanded" class="absolute" style="width: 180px; height: 180px; left: 50%; top: 50%; transform: translate(-50%, -50%);">
        <!-- 清屏 - 上 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          style="top: 8px; left: 50%; transform: translateX(-50%); transition: all 0.3s ease;"
          aria-label="清屏"
          @click.stop="handleClear"
        >
          <svg class="w-5 h-5 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>

        <!-- 输入 - 左上 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          style="top: 28px; left: 28px; transition: all 0.3s ease;"
          aria-label="输入命令"
          @click.stop="handleInput"
        >
          <svg class="w-5 h-5 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </button>

        <!-- Ctrl+C - 右上 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          style="top: 28px; right: 28px; transition: all 0.3s ease;"
          aria-label="发送 Ctrl+C"
          @click.stop="handleCtrlC"
        >
          <svg class="w-5 h-5 text-amber-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
        </button>

        <!-- 快捷键 - 左下 -->
        <button
          class="absolute w-11 h-11 rounded-full bg-white dark:bg-dark-700 shadow-md flex items-center justify-center"
          style="bottom: 28px; left: 28px; transition: all 0.3s ease;"
          aria-label="快捷键"
          @click.stop="handleShortcut"
        >
          <svg class="w-5 h-5 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
        </button>

        <!-- 设置 - 右下 -->
        <button
          class="absolute w-9 h-9 rounded-full bg-gray-100 dark:bg-dark-600 shadow-md flex items-center justify-center"
          style="bottom: 28px; right: 28px; transition: all 0.3s ease;"
          aria-label="设置"
          @click.stop="handleSettings"
        >
          <svg class="w-4 h-4 text-gray-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
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

  <!-- 设置弹窗 -->
  <SettingsModal
    :visible="showSettingsModal"
    @close="showSettingsModal = false"
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
import { ref, computed, onMounted, nextTick, watch, onUnmounted } from 'vue'
import { useInputAssistantStore } from '@/modules/shared/stores/inputAssistant'
import ShortcutPanel from './ShortcutPanel.vue'
import SettingsModal from './SettingsModal.vue'

// 终端实例类型定义
interface TerminalInstance {
  sendInput: (data: string) => void
  sendSpecialKey: (key: string) => void
}

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

// 屏幕尺寸（用于限制拖动范围）
const screenSize = ref({ width: window.innerWidth, height: window.innerHeight })

// 悬浮球位置样式
const floatingButtonStyle = computed(() => {
  const pos = store.position
  const edge = 16
  const size = store.settings.size

  // 限制在屏幕范围内
  const x = Math.max(edge, Math.min(pos.x, screenSize.value.width - size - edge))
  const y = Math.max(edge + 60, Math.min(pos.y, screenSize.value.height - size - edge))

  return {
    right: `${screenSize.value.width - x - size}px`,
    top: `${y}px`,
  }
})

// 根据悬浮球大小计算图标尺寸
const iconSizeClass = computed(() => {
  const size = store.settings.size
  if (size <= 40) return 'w-4 h-4'
  if (size <= 48) return 'w-5 h-5'
  return 'w-6 h-6'
})

// 设置弹窗状态
const showSettingsModal = ref(false)

// 手势状态
const lastTapTime = ref(0)
const touchStartPos = ref({ x: 0, y: 0 })
const touchStartTime = ref(0)

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

  // 记录触摸起始位置和时间
  const touch = e.touches[0]
  touchStartPos.value = { x: touch.clientX, y: touch.clientY }
  touchStartTime.value = Date.now()

  // 不立即进入拖动模式，等移动超过阈值后再进入
  isDragging.value = false
  dragStartPos.value = { x: touch.clientX, y: touch.clientY }
}

function onTouchMove(e: TouchEvent) {
  const touch = e.touches[0]
  const deltaX = touch.clientX - dragStartPos.value.x
  const deltaY = touch.clientY - dragStartPos.value.y
  const distance = Math.sqrt(deltaX * deltaX + deltaY * deltaY)
  const deltaTime = Date.now() - touchStartTime.value

  // 移动超过 10px 且时间超过 300ms 才进入拖动模式
  if (!isDragging.value && distance > 10 && deltaTime > 300) {
    isDragging.value = true
  }

  if (!isDragging.value) return

  e.preventDefault()

  // 计算新位置：基于当前触摸位置减去按钮半径
  const size = store.settings.size
  const newX = touch.clientX - size / 2
  const newY = touch.clientY - size / 2

  store.savePosition(newX, newY)
}

function onTouchEnd(e: TouchEvent) {
  const touchEndPos = { x: e.changedTouches[0].clientX, y: e.changedTouches[0].clientY }
  const deltaX = touchEndPos.x - touchStartPos.value.x
  const deltaY = touchEndPos.y - touchStartPos.value.y
  const distance = Math.sqrt(deltaX * deltaX + deltaY * deltaY)
  const deltaTime = Date.now() - touchStartTime.value

  // 已经进入拖动模式，保存位置后直接返回
  if (isDragging.value) {
    isDragging.value = false
    return
  }

  // 移动距离超过 30px 且时间 < 500ms，判定为滑动
  if (distance > 30 && deltaTime < 500) {
    handleSwipe(deltaX, deltaY)
    isDragging.value = false
    return
  }

  // 短按（时间 < 300ms 且移动距离小）- 点击事件，展开/收起��单
  if (deltaTime < 300 && distance < 10) {
    if (store.isExpanded) {
      store.collapse()
    } else {
      store.toggleExpanded()
    }
    isDragging.value = false
    return
  }

  // 长按（时间 >= 300ms 且移动距离小）- 显示输入框
  if (deltaTime >= 300 && distance < 10) {
    showInputModal.value = true
    isDragging.value = false
    return
  }

  // 其他情况（时间长但移动距离大）- 保存位置但不触发任何功能
  isDragging.value = false
}

// 处理滑动
function handleSwipe(deltaX: number, deltaY: number) {
  const { gestures } = store.settings

  // 判断滑动方向
  if (Math.abs(deltaX) > Math.abs(deltaY)) {
    // 水平滑动
    if (deltaX > 0 && gestures.swipeRight) {
      // 向右滑 - 输入
      handleInput()
    } else if (deltaX < 0 && gestures.swipeLeft) {
      // 向左滑 - 快捷键
      handleShortcut()
    }
  } else {
    // 垂直滑动
    if (deltaY > 0 && gestures.swipeDown) {
      // 向下滑 - 清屏
      handleClear()
    } else if (deltaY < 0 && gestures.swipeUp) {
      // 向上滑 - Ctrl+C
      handleCtrlC()
    }
  }
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
  if (inputText.value.trim() && props.terminalInstance) {
    props.terminalInstance.sendInput(inputText.value)
    inputText.value = ''
    showInputModal.value = false
  }
}

function executeText() {
  if (inputText.value.trim() && props.terminalInstance) {
    const terminal = props.terminalInstance
    terminal.sendInput(inputText.value)
    setTimeout(() => {
      terminal.sendSpecialKey('enter')
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

// 设置
function handleSettings() {
  store.collapse()
  showSettingsModal.value = true
}

function handleShortcutSelect(key: string) {
  if (props.isConnected && props.terminalInstance) {
    props.terminalInstance.sendSpecialKey(key)
  }
}

// 清屏
function handleClear() {
  store.collapse()
  props.terminalRef?.value?.clear()
}

// Ctrl+C
function handleCtrlC() {
  store.collapse()
  if (props.isConnected && props.terminalInstance) {
    props.terminalInstance.sendSpecialKey('ctrl_c')
  }
}

// 监听窗口大小变化
onMounted(() => {
  window.addEventListener('resize', handleResize)
})

onUnmounted(() => {
  window.removeEventListener('resize', handleResize)
})

function handleResize() {
  screenSize.value = { width: window.innerWidth, height: window.innerHeight }
}
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

.expand-enter-from button {
  transform: translate(-50%, -50%) scale(0) !important;
}

.expand-leave-to button {
  transform: translate(-50%, -50%) scale(0) !important;
}

/* 按钮延迟进入动画 */
.expand-enter-active button {
  animation: menuItemFadeIn 0.3s ease forwards;
  opacity: 0;
}

.expand-enter-active button:nth-child(1) { animation-delay: 0ms; }
.expand-enter-active button:nth-child(2) { animation-delay: 50ms; }
.expand-enter-active button:nth-child(3) { animation-delay: 100ms; }
.expand-enter-active button:nth-child(4) { animation-delay: 150ms; }
.expand-enter-active button:nth-child(5) { animation-delay: 200ms; }

@keyframes menuItemFadeIn {
  from {
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.5);
  }
  to {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}
</style>
