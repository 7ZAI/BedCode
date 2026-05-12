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
        @resize="handleTerminalResize"
      />
    </div>

    <!-- Input Assistant 悬浮球 -->
    <InputAssistant
      :terminal-ref="terminalRef"
      :terminal-instance="terminal"
      :is-connected="isConnectedValue"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, onActivated, onDeactivated, inject, type Ref } from 'vue'

// 定义组件名称，用于 KeepAlive 缓存
defineOptions({
  name: 'MobileTerminal',
})

import { useRouter, useRoute } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'
import MobileTerminal from '@/components/mobile/MobileTerminal.vue'
import InputAssistant from '@/components/mobile/InputAssistant.vue'

const router = useRouter()
const route = useRoute()

// 注入屏幕方向
const isLandscape = inject<Ref<boolean>>('isLandscape', ref(false))
const isLandscapeValue = computed(() => isLandscape.value)

const connection = useRemoteConnection()

// 使用统一的连接状态（computed 会自动解包）
const isConnectedValue = connection.isConnected

// 创建自定义的连接包装，禁用自动清理
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: isConnectedValue,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  // 覆盖 setReconnectCallback 为空操作，防止自动清理
  setReconnectCallback: () => {},
  addDisconnectCallback: () => {},
})

const terminalRef = ref<any>(null)

// 会话名称 - 显示当前活跃会话的名称，如果没有则显示设备名称
const sessionName = computed(() => {
  const currentSession = terminal.sessions.value.find(
    s => s.id === terminal.currentSessionId.value
  )
  return currentSession?.name || connection.currentDevice.value?.name || 'Claude Code'
})

// 清空终端
function handleClear() {
  terminalRef.value?.clear()
}

// Terminal ready handler
function onTerminalReady() {
  console.log('Mobile terminal ready')
}

// Terminal resize handler - 发送终端尺寸到后端
function handleTerminalResize(cols: number, rows: number) {
  if (terminal.currentSessionId.value) {
    connection.sendMessage('control', {
      action: { type: 'resize_session', session_id: terminal.currentSessionId.value, cols, rows }
    })
  }
}

// Terminal clear handler
function onTerminalClear() {
  terminal.clearOutput()
}

// KeepAlive 恢复时的处理
// 当组件被 KeepAlive 缓存后再次激活时，需要恢复活跃会话状态
onActivated(async () => {
  // 检查是否有已订阅的会话
  if (terminal.joinedSessions.value.size > 0 && !terminal.activeSessionId.value) {
    // 如果有已订阅的会话但没有活跃会话，恢复第一个
    const firstSessionId = terminal.joinedSessions.value.values().next().value
    if (firstSessionId) {
      // 恢复活跃会话状态，但不重新订阅（已订阅）
      terminal.activeSessionId.value = firstSessionId
      terminal.currentSessionId.value = firstSessionId
      // 恢复活跃会话 ID 标记到 connection
      connection.activeSessionId.value = firstSessionId
      console.log('Restored active session:', firstSessionId)
    }
  }

  // 如果已有活跃会话，确保 connection.activeSessionId 也同步
  if (terminal.activeSessionId.value) {
    connection.activeSessionId.value = terminal.activeSessionId.value
  }
})

// KeepAlive 停用时的处理
onDeactivated(() => {
  // 组件被缓存时，不需要做任何清理
  // 保留活跃会话状态，供恢复时使用
  console.log('TerminalView deactivated, keeping session state')
})

onMounted(async () => {
  const deviceId = route.params.deviceId as string
  const sessionId = route.query.sessionId as string | undefined

  // 启用自动重连恢复
  terminal.enableAutoReconnect()

  // 注册断开连接回调，清除会话状态
  connection.addDisconnectCallback(() => {
    terminal.clearAllSessionState()
  })

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
      try {
        await terminal.joinSession(sessionId)
        connection.activeSessionId.value = terminal.currentSessionId.value
      } catch (e) {
        console.error('Failed to join session:', e)
        alert((e as Error).message)
        router.push('/mobile/sessions')
      }
    } else if (terminal.sessions.value.length > 0) {
      // 未指定会话时，自动选择第一个
      try {
        await terminal.joinSession(terminal.sessions.value[0].id)
        connection.activeSessionId.value = terminal.currentSessionId.value
      } catch (e) {
        console.error('Failed to join session:', e)
        alert((e as Error).message)
        router.push('/mobile/sessions')
      }
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
