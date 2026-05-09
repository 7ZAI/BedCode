<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3 flex items-center gap-3 shrink-0" style="padding-top: calc(var(--safe-area-inset-top, 0px) + 12px);">
      <button @click="goBack" class="p-2 -ml-2">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="flex-1">
        <h1 class="font-semibold">{{ deviceName }}</h1>
        <div class="flex items-center gap-1.5 text-xs">
          <div
            :class="[
              'w-1.5 h-1.5 rounded-full',
              connection.isConnected.value ? 'bg-green-500' : 'bg-red-500'
            ]"
          ></div>
          <span class="text-dark-400">{{ connection.isConnected.value ? '已连接' : '未连接' }}</span>
        </div>
      </div>
      <button
        class="p-2 rounded-lg bg-dark-700 text-dark-300"
        @click="handleClear"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output (动态高度适配键盘) -->
    <div
      class="flex-1 overflow-hidden"
      :style="{ height: terminalHeight + 'px' }"
    >
      <MobileTerminal
        ref="terminalRef"
        :output="terminal.outputBuffer.value"
        @ready="onTerminalReady"
        @clear="onTerminalClear"
      />
    </div>

    <!-- Input Bar - 键盘弹出时使用 padding-bottom 适配 -->
    <InputBar
      ref="inputBarRef"
      :is-connected="connection.isConnected.value"
      :show-status="true"
      :keyboard-height="keyboardHeight"
      placeholder="输入消息..."
      @submit="handleSendInput"
      @special-key="handleSendSpecialKey"
      @focus="onInputFocus"
      @blur="onInputBlur"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'
import { useKeyboardAvoidance } from '@/composables/useKeyboardAvoidance'
import MobileTerminal from '@/components/mobile/MobileTerminal.vue'
import InputBar from '@/components/mobile/InputBar.vue'

const router = useRouter()
const route = useRoute()

const { keyboardHeight } = useKeyboardAvoidance()

// 终端区域动态高度
const terminalHeight = computed(() => {
  // 当键盘弹出时，减少终端高度；键盘收起时恢复
  return `calc(100% - ${keyboardHeight.value}px - 72px)` // 72px 是 InputBar 的高度
})

const connection = useRemoteConnection()
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
})

const terminalRef = ref<InstanceType<typeof MobileTerminal> | null>(null)
const inputBarRef = ref<InstanceType<typeof InputBar> | null>(null)

const deviceName = computed(() => connection.currentDevice.value?.name || 'Claude Code')

// 监听等待输入状态
watch(() => terminal.isWaitingInput.value, (waiting) => {
  if (waiting) {
    inputBarRef.value?.focus()
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

  // 加载远程会话
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
})

onUnmounted(async () => {
  // 禁用自动重连并离开会话
  terminal.disableAutoReconnect()
  await terminal.leaveSession()
  // 清除活跃会话 ID，通知其他视图连接仍存在但会话已离开
  connection.activeSessionId.value = null
})

function goBack() {
  router.push('/mobile/devices')
}

function handleSendInput(text: string) {
  terminal.sendInput(text)
  // 无需手动回显，xterm.js 会通过 PTY echo 自动显示输入
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
