<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header - 横屏时更紧凑 -->
    <header
      class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-2 flex items-center gap-2 shrink-0"
      :class="{ 'py-1': isLandscapeValue, 'py-3 pb-3': !isLandscapeValue }"
      :style="{ paddingTop: isLandscapeValue ? '8px' : '12px' }"
    >
      <button @click="goBack" class="p-2 -ml-2">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="flex-1 min-w-0">
        <h1 class="font-semibold truncate" :class="isLandscapeValue ? 'text-sm' : ''">{{ sessionName }}</h1>
      </div>
      <div class="flex items-center gap-1.5 text-xs">
        <div
          :class="[
            'w-1.5 h-1.5 rounded-full',
            connection.isConnected.value ? 'bg-green-500' : 'bg-red-500'
          ]"
        ></div>
        <span class="text-gray- dark:text-dark-400">{{ connection.isConnected.value ? '已连接' : '未连接' }}</span>
      </div>
      <button
        class="p-2 rounded-lg bg-gray-100 dark:bg-dark-700 text-gray- dark:text-dark-300"
        @click="handleClear"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output (使用 flex 填充剩余空间) -->
    <div class="flex-1 overflow-hidden min-h-0">
      <MobileTerminal
        ref="terminalRef"
        :output="terminal.outputBuffer.value"
        @ready="onTerminalReady"
        @clear="onTerminalClear"
      />
    </div>

    <!-- Input Bar - 键盘弹出时使用 fixed 定位 -->
    <InputBar
      ref="inputBarRef"
      :is-connected="isConnectedValue"
      :show-status="false"
      :keyboard-height="keyboardHeight"
      :is-landscape="isLandscapeValue"
      placeholder="输入消息..."
      @submit="handleSendInput"
      @execute="handleExecuteInput"
      @special-key="handleSendSpecialKey"
      @focus="onInputFocus"
      @blur="onInputBlur"
    />
    <!-- Input Assistant 悬浮球 -->
    <InputAssistant
      :terminal-ref="terminalRef"
      :terminal-instance="terminal"
      :is-connected="isConnectedValue"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, inject, type Ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'
import { useKeyboardAvoidance } from '@/composables/useKeyboardAvoidance'
import MobileTerminal from '@/components/mobile/MobileTerminal.vue'
import InputBar from '@/components/mobile/InputBar.vue'
import InputAssistant from '@/components/mobile/InputAssistant.vue'

const router = useRouter()
const route = useRoute()

// 注入屏幕方向
const isLandscape = inject<Ref<boolean>>('isLandscape', ref(false))
const isLandscapeValue = computed(() => isLandscape.value)

const { keyboardHeight } = useKeyboardAvoidance()

// 终端区域动态高度（使用 calc 计算固定高度）
const terminalHeight = computed(() => {
  // Header 高度
  const headerHeight = isLandscapeValue.value ? 44 : 52 // 横屏时更紧凑
  // InputBar 基础高度（键盘收起时）
  const inputBarHeight = isLandscapeValue.value ? 56 : 100 // 横屏时只显示输入框
  // 底部安全区域
  const safeAreaBottom = 'env(safe-area-inset-bottom, 0px)'

  const kbHeight = keyboardHeight.value

  if (kbHeight > 0) {
    // 键盘弹出时：键盘高度由 InputBar 的 fixed 定位处理
    return `calc(100% - ${headerHeight}px - ${safeAreaBottom})`
  }
  // 键盘收起时
  return `calc(100% - ${headerHeight}px - ${inputBarHeight}px - ${safeAreaBottom})`
})

const connection = useRemoteConnection()

// 解包 isConnected Ref 为布尔值
const isConnectedValue = computed(() => connection.isConnected.value)

// 创建自定义的连接包装，禁用自动清理
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: isConnectedValue,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  // 覆盖 setReconnectCallback 为空操作，防止自动清理
  setReconnectCallback: () => {},
})

const terminalRef = ref<InstanceType<typeof MobileTerminal> | null>(null)
const inputBarRef = ref<InstanceType<typeof InputBar> | null>(null)

// 会话名称 - 显示当前活跃会话的名称，如果没有则显示设备名称
const sessionName = computed(() => {
  const currentSession = terminal.sessions.value.find(
    s => s.id === terminal.currentSessionId.value
  )
  return currentSession?.name || connection.currentDevice.value?.name || 'Claude Code'
})

// 监听等待输入状态
watch(() => terminal.isWaitingInput.value, (waiting) => {
  if (waiting) {
    inputBarRef.value?.focus()
  }
})

// 监听连接状态变化，认证完成后自动加载会话
watch(() => connection.state.value.status, async (status) => {
  if (status === 'paired' && terminal.sessions.value.length === 0) {
    await terminal.loadSessions()
  }
})

// 输入框获得焦点时，确保终端能正确滚动
function onInputFocus() {
  // 短暂延迟后滚动到底部，确保键盘已弹出
  setTimeout(() => {
    terminalRef.value?.scrollToBottom()
  }, 300)
}

// 输入框失去焦点时，收起键盘
function onInputBlur() {
  // 失去焦点时键盘会自动收起，这里可以做一些清理
}

// Terminal ready handler
function onTerminalReady() {
  console.log('Mobile terminal ready')
}

// Terminal clear handler
function onTerminalClear() {
  terminal.clearOutput()
}

// 清空终端
function handleClear() {
  terminalRef.value?.clear()
}

onMounted(async () => {
  const deviceId = route.params.deviceId as string
  const sessionId = route.query.sessionId as string | undefined

  // 启用自动重连恢复
  terminal.enableAutoReconnect()

  // 如果未连接，先连接
  if (connection.state.value.status !== 'connected' && connection.state.value.status !== 'paired') {
    const device = connection.pairedDevices.value.find(d => d.id === deviceId)
    if (device) {
      try {
        await connection.connect(device)
      } catch (error) {
        console.error('Failed to connect:', error)
        return
      }
    }
  }

  // 只有在已认证状态下才加载远程会话
  if (connection.state.value.status === 'paired') {
    await terminal.loadSessions()

    if (sessionId) {
      // 通过查询参数直接加入指定会话
      await terminal.joinSession(sessionId)
      connection.activeSessionId.value = terminal.currentSessionId.value
    } else if (terminal.sessions.value.length > 0) {
      // 未指定会话时，自动选择第一个
      await terminal.joinSession(terminal.sessions.value[0].id)
      connection.activeSessionId.value = terminal.currentSessionId.value
    }
  }
})

onUnmounted(async () => {
  // 只设置活跃会话为 null，不断开订阅（后台继续接收数据）
  terminal.setActiveSession(null)
  // 清除活跃会话 ID 标记
  connection.activeSessionId.value = null
})

function goBack() {
  // 只设置活跃会话为 null，不断开订阅（后台继续接收数据）
  terminal.setActiveSession(null)
  router.push('/mobile/sessions')
}

function handleSendInput(text: string) {
  terminal.sendInput(text)
  // 无需手动回显，xterm.js 会通过 PTY echo 自动显示输入
}

function handleExecuteInput(text: string) {
  // 执行：发送文本并自动发送 Enter
  terminal.sendInput(text)
  // 延迟发送 Enter，确保命令先到达
  setTimeout(() => {
    terminal.sendSpecialKey('enter')
  }, 50)
}

function handleSendSpecialKey(key: string) {
  terminal.sendSpecialKey(key)
}

async function handleSelectSession(sessionId: string) {
  await terminal.joinSession(sessionId)
  // 同步活跃会话 ID
  connection.activeSessionId.value = terminal.currentSessionId.value
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
