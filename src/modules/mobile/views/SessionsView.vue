<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-4 pb-3 flex items-center justify-between" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold">会话</h1>
      <button
        v-if="isConnected"
        class="p-2 rounded-lg active:bg-gray-100 dark:active:bg-dark-700 transition-colors"
        :class="{ 'opacity-50': isRefreshing }"
        :disabled="isRefreshing"
        @click="refreshSessions"
        title="刷新会话"
      >
        <svg
          class="w-5 h-5 text-gray- dark:text-dark-400"
          :class="{ 'animate-spin': isRefreshing }"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
    </header>

    <!-- Content -->
    <div class="flex-1 overflow-auto">
      <!-- Empty state -->
      <div v-if="!isConnected" class="flex flex-col items-center justify-center h-full text-gray-400">
        <svg class="w-16 h-16 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-center">未连接到桌面端<br/>请先在"设备"页面连接</p>
      </div>

      <!-- Sessions list -->
      <div v-else class="p-4 space-y-3">
        <div v-if="isLoading" class="flex justify-center py-8">
          <div class="w-8 h-8 border-2 border-primary-400 border-t-transparent rounded-full animate-spin" />
        </div>

        <div v-else-if="sessions.length === 0" class="text-center text-gray-400 py-8">
          暂无运行中的会话
        </div>

        <SessionCard
          v-for="session in sessions"
          :key="session.id"
          :session="session"
          @click="handleSessionClick(session)"
          @stop="handleStopSession(session)"
        />
      </div>
    </div>

    <!-- Connection info -->
    <div v-if="isConnected" class="px-4 py-2 bg-white dark:bg-dark-800 border-t border-gray-200 dark:border-dark-700">
      <span class="text-gray- dark:text-dark-400 text-xs font-medium">{{ currentDeviceName }} · {{ sessions.length }} 个会话</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onActivated } from 'vue'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/modules/shared/composables/useMobileConnection'
import { wsLoadSessions, wsStopSession } from '@/modules/shared/composables/useMobileCommands'
import SessionCard from '@/modules/mobile/components/SessionCard.vue'

const router = useRouter()
const connection = useMobileConnection()

// 连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前设备名称
const currentDeviceName = computed(() => connection.currentDevice.value?.name || '已连接')

// 会话列表
const sessions = ref<any[]>([])
const isLoading = ref(false)
const isRefreshing = ref(false)

function handleSessionClick(session: any) {
  connection.activeSessionId.value = session.id

  router.push({
    name: 'mobile-terminal',
    params: { id: currentDeviceName.value || 'default' },
  })
}

async function handleStopSession(session: any) {
  const name = session.name || session.id
  if (!window.confirm(`确定停止会话 "${name}" 吗？`)) return
  try {
    await wsStopSession(session.id)
    sessions.value = sessions.value.filter(s => s.id !== session.id)
  } catch (e) {
    console.error('[SessionsView] Failed to stop session:', e)
  }
}

async function refreshSessions() {
  if (!isConnected.value) return
  isRefreshing.value = true
  isLoading.value = true
  try {
    sessions.value = await wsLoadSessions()
  } catch (e) {
    console.error('[SessionsView] Failed to load sessions:', e)
  } finally {
    isLoading.value = false
    isRefreshing.value = false
  }
}

onActivated(() => {
  refreshSessions()
})

onMounted(async () => {
  await refreshSessions()
})
</script>