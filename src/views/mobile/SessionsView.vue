<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-4 pb-3" style="padding-top: 12px;">
      <div class="flex items-center justify-between">
        <h1 class="text-lg font-semibold">会话</h1>
        <button
          v-if="connection.isConnected.value"
          class="p-2 rounded-lg active:bg-gray-100 dark:bg-dark-700 transition-colors"
          :class="{ 'opacity-50': isLoading }"
          :disabled="isLoading"
          @click="refreshSessions"
          title="刷新会话"
        >
          <svg
            class="w-5 h-5 text-gray- dark:text-dark-400"
            :class="{ 'animate-spin': isLoading }"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Not Connected -->
    <div v-if="!isConnected" class="flex-1 flex items-center justify-center p-8">
      <div class="text-center">
        <svg class="w-16 h-16 mx-auto text-gray- dark:text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
        <p class="text-gray- dark:text-dark-300 font-medium mb-2">未连接设备</p>
        <p class="text-gray- dark:text-dark-500 text-sm mb-4">请先在"连接"页面连接到桌面端</p>
        <button
          class="bg-primary-600 text-white px-6 py-2.5 rounded-xl text-sm font-medium active:bg-primary-700"
          @click="$router.push({ name: 'mobile-devices' })"
        >
          前往连接
        </button>
      </div>
    </div>

    <!-- Connected: Loading -->
    <div v-else-if="isLoading && !hasLoadedSessions" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <div class="w-8 h-8 border-2 border-primary-400 border-t-transparent rounded-full animate-spin mx-auto mb-3" />
        <p class="text-gray- dark:text-dark-500 text-sm">加载会话中...</p>
      </div>
    </div>

    <!-- Connected: Empty -->
    <div v-else-if="!isLoading && sessions.length === 0 && hasLoadedSessions" class="flex-1 flex items-center justify-center p-8">
      <div class="text-center">
        <svg class="w-16 h-16 mx-auto text-gray- dark:text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-gray- dark:text-dark-300 font-medium mb-2">暂无活跃会话</p>
        <p class="text-gray- dark:text-dark-500 text-sm">前往"连接"页面启动新会话</p>
      </div>
    </div>

    <!-- Connected: Session List -->
    <div v-else class="flex-1 overflow-auto p-4">
      <!-- Connection info bar -->
      <div class="flex items-center gap-2 mb-4">
        <div class="w-2 h-2 rounded-full bg-green-500 animate-pulse"></div>
        <span class="text-gray- dark:text-dark-400 text-xs font-medium">{{ connection.currentDevice.value?.name || '已连接' }} · {{ sessions.length }} 个会话</span>
      </div>

      <div class="space-y-3">
        <SessionCard
          v-for="session in sessions"
          :key="session.id"
          :session="session"
          @click="handleSessionClick(session)"
          @stop="handleStopSession(session)"
          @delete="handleRemoveSession(session)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal, type RemoteSession } from '@/composables/useRemoteTerminal'
import SessionCard from '@/components/mobile/SessionCard.vue'

const router = useRouter()
const connection = useRemoteConnection()
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
  addDisconnectCallback: connection.addDisconnectCallback,
})

// 使用统一的连接状态
const isConnected = connection.isConnected

const sessions = computed(() => terminal.sessions.value)
const isLoading = computed(() => terminal.isLoading.value)
const hasLoadedSessions = ref(false) // 标记是否已经加载过数据（防止闪烁）

function handleSessionClick(session: RemoteSession) {
  const deviceId = connection.currentDevice.value?.id
  if (!deviceId) return

  connection.activeSessionId.value = session.id

  router.push({
    name: 'mobile-terminal',
    params: { deviceId },
    query: { sessionId: session.id },
  })
}

async function handleStopSession(session: RemoteSession) {
  try {
    await terminal.stopSession(session.id)
  } catch (e) {
    console.error('Failed to stop session:', e)
  }
}

async function handleRemoveSession(session: RemoteSession) {
  try {
    await terminal.removeSession(session.id)
  } catch (e) {
    console.error('Failed to remove session:', e)
  }
}

async function refreshSessions() {
  if (!connection.isConnected.value || connection.state.value.status !== 'paired') return
  hasLoadedSessions.value = false  // 开始刷新时重置标记
  await terminal.loadSessions()
}

onMounted(() => {
  // 启用自动重连恢复
  terminal.enableAutoReconnect()

  // 注册断开连接回调，清除会话状态
  connection.addDisconnectCallback(() => {
    terminal.clearAllSessionState()
    hasLoadedSessions.value = false  // 重置标记，重新连接后显示加载状态
  })

  // 只有在已认证状态下才加载会话
  if (connection.isConnected.value && connection.state.value.status === 'paired') {
    terminal.loadSessions()
  }
})

// 监听加载完成，防止数据闪烁
watch(isLoading, (loading, prevLoading) => {
  // 从加载中变为非加载中，表示一次加载周期完成
  if (prevLoading === true && loading === false) {
    hasLoadedSessions.value = true
  }
})

// 监听连��和配对状态变化，自动刷新会话列表
watch([() => connection.isConnected.value, () => connection.state.value.status], async ([connected, status]) => {
  if (connected && status === 'paired') {
    await terminal.loadSessions()
  }
})
</script>
