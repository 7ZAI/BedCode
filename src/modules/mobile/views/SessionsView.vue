<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-4 pb-3 flex items-center justify-between" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold">会话</h1>
      <button
        v-if="isConnected"
        class="p-2 rounded-lg active:bg-gray-100 dark:active:bg-dark-700 transition-colors"
        @click="refreshSessions"
        title="刷新会话"
      >
        <svg
          class="w-5 h-5 text-gray- dark:text-dark-400"
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
        <div v-if="sessions.length === 0" class="text-center text-gray-400 py-8">
          暂无运行中的会话
        </div>

        <SessionCard
          v-for="session in sessions"
          :key="session.id"
          :session="session"
          @click="handleSessionClick(session)"
          @stop="handleStopSession(session)"
          @delete="handleDeleteSession(session)"
        />
      </div>
    </div>

    <!-- Connection info -->
    <div v-if="isConnected" class="px-4 py-2 bg-white dark:bg-dark-800 border-t border-gray-200 dark:border-dark-700">
      <span class="text-gray- dark:text-dark-400 text-xs font-medium">{{ currentDeviceName }} · {{ sessions.length }} 个会话</span>
    </div>

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" title="确认停止会话" size="sm">
      <p class="text-gray-600 dark:text-dark-300">
        确定要停止会话 "<span class="text-white font-medium">{{ pendingSession?.name || pendingSession?.id }}</span>" 吗？
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirm = false">取消</Button>
          <Button variant="danger" :loading="isStopping" @click="confirmStop">停止</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirmation Modal -->
    <Modal v-model="showDeleteConfirm" title="确认删除会话" size="sm">
      <p class="text-gray-600 dark:text-dark-300">
        确定要删除会话 "<span class="text-white font-medium">{{ pendingSession?.name || pendingSession?.id }}</span>" 吗？此操作不可恢复。
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirm = false">取消</Button>
          <Button variant="danger" :loading="isDeleting" @click="confirmDelete">删除</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onActivated } from 'vue'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { wsStopSession, wsRemoveSession } from '@/modules/mobile/composables/useMobileCommands'
import { useToast } from '@/modules/shared/composables/useToast'
import SessionCard from '@/modules/mobile/components/SessionCard.vue'
import Modal from '@/modules/shared/components/Modal.vue'
import Button from '@/modules/shared/components/Button.vue'

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()

// 连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前设备名称
const currentDeviceName = computed(() => connection.currentDevice.value?.name || '已连接')

// 使用全局会话列表（与同步事件同步）
const sessions = computed(() => connection.activeSessions.value)

// 停止确认弹窗
const showStopConfirm = ref(false)
const pendingSession = ref<any>(null)
const isStopping = ref(false)

// 删除确认弹窗
const showDeleteConfirm = ref(false)
const isDeleting = ref(false)

function handleSessionClick(session: any) {
  connection.activeSessionId.value = session.id

  router.push({
    name: 'mobile-terminal',
    params: { id: currentDeviceName.value || 'default' },
  })
}

function handleStopSession(session: any) {
  pendingSession.value = session
  showStopConfirm.value = true
}

async function confirmStop() {
  if (!pendingSession.value) return
  isStopping.value = true
  try {
    await wsStopSession(pendingSession.value.id)
    // 全局状态由同步事件自动更新，无需手动移除
    showStopConfirm.value = false
    pendingSession.value = null
  } catch (e) {
    console.error('[SessionsView] Failed to stop session:', e)
    toast.error('停止会话失败')
  } finally {
    isStopping.value = false
  }
}

function handleDeleteSession(session: any) {
  pendingSession.value = session
  showDeleteConfirm.value = true
}

async function confirmDelete() {
  if (!pendingSession.value) return
  isDeleting.value = true
  try {
    await wsRemoveSession(pendingSession.value.id)
    // 全局状态由同步事件自动更新，无需手动移除
    showDeleteConfirm.value = false
    pendingSession.value = null
  } catch (e) {
    console.error('[SessionsView] Failed to delete session:', e)
    toast.error('删除会话失败')
  } finally {
    isDeleting.value = false
  }
}

// 刷新会话列表（从桌面端拉取最新数据）
async function refreshSessions() {
  if (!isConnected.value) return
  try {
    await connection.loadActiveSessions()
  } catch (e) {
    console.error('[SessionsView] Failed to load sessions:', e)
    toast.error('加载会话列表失败')
  }
}

onActivated(() => {
  // 全局状态由同步事件自动维护，无需手动加载
})

onMounted(async () => {
  // 全局状态由同步事件自动维护，无需手动加载
})
</script>