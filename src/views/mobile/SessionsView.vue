<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3" style="padding-top: calc(var(--safe-area-inset-top, 0px) + 12px);">
      <div class="flex items-center justify-between">
        <h1 class="text-lg font-semibold">会话</h1>
        <button
          v-if="connection.isConnected.value"
          class="text-dark-500 text-xs"
          :class="{ 'opacity-50': isLoading }"
          :disabled="isLoading"
          @click="refreshSessions"
        >
          {{ isLoading ? '刷新中...' : '刷新' }}
        </button>
      </div>
    </header>

    <!-- Not Connected -->
    <div v-if="!connection.isConnected.value" class="flex-1 flex items-center justify-center p-8">
      <div class="text-center">
        <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
        <p class="text-dark-300 font-medium mb-2">未连接设备</p>
        <p class="text-dark-500 text-sm mb-4">请先在"连接"页面连接到桌面端</p>
        <button
          class="bg-primary-600 text-white px-6 py-2.5 rounded-xl text-sm font-medium active:bg-primary-700"
          @click="$router.push({ name: 'mobile-devices' })"
        >
          前往连接
        </button>
      </div>
    </div>

    <!-- Connected: Loading -->
    <div v-else-if="isLoading && sessions.length === 0" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <div class="w-8 h-8 border-2 border-primary-400 border-t-transparent rounded-full animate-spin mx-auto mb-3" />
        <p class="text-dark-500 text-sm">加载会话中...</p>
      </div>
    </div>

    <!-- Connected: Empty -->
    <div v-else-if="sessions.length === 0" class="flex-1 flex items-center justify-center p-8">
      <div class="text-center">
        <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-dark-300 font-medium mb-2">暂无活跃会话</p>
        <p class="text-dark-500 text-sm">前往"连接"页面启动新会话</p>
      </div>
    </div>

    <!-- Connected: Session List -->
    <div v-else class="flex-1 overflow-auto p-4">
      <!-- Connection info bar -->
      <div class="flex items-center gap-2 mb-3">
        <div class="w-2 h-2 rounded-full bg-green-500"></div>
        <span class="text-dark-400 text-xs">{{ connection.currentDevice.value?.name || '已连接' }} · {{ sessions.length }} 个会话</span>
      </div>

      <div class="space-y-2">
        <div
          v-for="session in sessions"
          :key="session.id"
          class="bg-dark-800 rounded-xl active:bg-dark-700 transition-colors overflow-hidden"
          :class="{ 'opacity-60': session.status === 'stopped' }"
        >
          <div class="flex">
            <!-- Status color bar -->
            <div
              :class="[
                'w-1 shrink-0',
                session.status === 'running' ? 'bg-green-500' :
                session.status === 'waiting_input' ? 'bg-yellow-500' : 'bg-red-500'
              ]"
            ></div>

            <!-- Content -->
            <div class="flex-1 p-4 min-w-0" @click="handleSessionClick(session)">
              <div class="flex items-start justify-between">
                <div class="flex-1 min-w-0">
                  <p class="font-medium truncate">{{ session.name }}</p>
                  <div class="flex items-center gap-2 mt-1">
                    <span
                      :class="[
                        'text-xs px-1.5 py-0.5 rounded-full',
                        session.status === 'running' ? 'bg-green-900/50 text-green-400' :
                        session.status === 'waiting_input' ? 'bg-yellow-900/50 text-yellow-400' : 'bg-red-900/50 text-red-400'
                      ]"
                    >
                      {{ statusLabel(session.status) }}
                    </span>
                    <span class="text-dark-500 text-xs">{{ elapsedTime(session) }}</span>
                  </div>
                </div>
                <svg class="w-5 h-5 text-dark-400 shrink-0 ml-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                </svg>
              </div>
            </div>

            <!-- Stop button (running/waiting sessions only) -->
            <button
              v-if="session.status !== 'stopped'"
              class="px-3 flex items-center justify-center active:bg-dark-700"
              @click.stop="handleStopSession(session)"
            >
              <svg class="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 6h12v12H6z" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal, type RemoteSession } from '@/composables/useRemoteTerminal'

const router = useRouter()
const connection = useRemoteConnection()
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
})

const sessions = computed(() => terminal.sessions.value)
const isLoading = computed(() => terminal.isLoading.value)

function statusLabel(status: string): string {
  switch (status) {
    case 'running': return '运行中'
    case 'waiting_input': return '等待输入'
    case 'stopped': return '已停止'
    default: return status
  }
}

function elapsedTime(session: RemoteSession): string {
  const start = session.startedAt || session.createdAt
  if (!start) return ''
  const elapsed = Date.now() - new Date(start).getTime()
  const seconds = Math.floor(elapsed / 1000)
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${minutes % 60}m`
}

function handleSessionClick(session: RemoteSession) {
  const deviceId = connection.currentDevice.value?.id
  if (!deviceId) return

  connection.activeSessionId.value = session.id

  router.push({
    name: 'mobile-terminal',
    params: { id: deviceId },
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

async function refreshSessions() {
  if (!connection.isConnected.value) return
  await terminal.loadSessions()
}

onMounted(() => {
  if (connection.isConnected.value) {
    terminal.loadSessions()
  }
})
</script>
