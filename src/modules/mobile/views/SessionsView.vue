<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-4 pb-3" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold">会话</h1>
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
import { computed, ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/modules/shared/composables/useMobileConnection'
import SessionCard from '@/modules/mobile/components/SessionCard.vue'

const router = useRouter()
const connection = useMobileConnection()

// 连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前设备名称
const currentDeviceName = computed(() => connection.currentDevice.value?.name || '已连接')

// 会话列表（临时空数组）
const sessions = ref<any[]>([])
const isLoading = ref(false)

function handleSessionClick(session: any) {
  const deviceId = connection.currentDevice.value?.id
  if (!deviceId) return

  connection.activeSessionId.value = session.id

  router.push({
    name: 'mobile-terminal',
    params: { deviceId },
    query: { sessionId: session.id },
  })
}

async function handleStopSession(session: any) {
  // TODO: 实现停止会话
  console.log('Stop session:', session.id)
}

async function refreshSessions() {
  if (!isConnected.value) return
  // TODO: 实现加载会话
  // sessions.value = await connection.loadSessions()
}

onMounted(async () => {
  await refreshSessions()
})
</script>