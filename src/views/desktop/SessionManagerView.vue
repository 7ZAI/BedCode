<template>
  <div class="h-full flex">
    <!-- Left Panel: All Sessions List -->
    <div class="w-full flex flex-col bg-gray-50 dark:bg-dark-900 border-r border-gray-200 dark:border-dark-700">
      <!-- Header -->
      <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-6 py-3 h-12 flex items-center">
        <div class="flex items-center justify-between w-full">
          <h2 class="text-lg font-semibold">会话管理</h2>
          <Button variant="ghost" @click="refreshSessions">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </Button>
        </div>
      </header>

      <!-- Session List -->
      <div class="flex-1 overflow-auto p-4">
        <!-- Loading State -->
        <div v-if="isLoading" class="text-center py-12">
          <Spinner size="xl" color="primary" class="mb-4" />
          <p class="text-gray- dark:text-dark-400">加载中...</p>
        </div>

        <!-- Empty State -->
        <div v-else-if="allSessions.length === 0" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-gray- dark:text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-gray- dark:text-dark-400">暂无会话</p>
          <p class="text-gray- dark:text-dark-500 text-sm mt-2">在"会话配置"页面启动会话</p>
          <Button variant="primary" class="mt-4" @click="goToSessionConfig">
            前往会话配置
          </Button>
        </div>

        <!-- Session List (including stopped) -->
        <div v-else class="space-y-3">
          <SessionItem
            v-for="session in allSessions"
            :key="session.id"
            :session="session"
            @view="viewSession(session)"
            @stop="confirmStopSession(session)"
            @restart="restartSession(session)"
            @delete="confirmDeleteSession(session)"
          />
        </div>
      </div>
    </div>

    <!-- Stop Confirm Dialog -->
    <Modal v-model="showStopConfirmDialog" title="确认停止会话" size="sm">
      <p class="text-gray- dark:text-dark-300">确定要停止会话 "<span class="text-white font-medium">{{ pendingSession?.name }}</span>" 吗？</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirmDialog = false">取消</Button>
          <Button variant="danger" :loading="isOperating" @click="confirmStop">停止</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirm Dialog -->
    <Modal v-model="showDeleteConfirmDialog" title="确认删除会话" size="sm">
      <p class="text-gray- dark:text-dark-300">
        会话 "<span class="text-white font-medium">{{ pendingSession?.name }}</span>" 仍在运行，将先停止再删除。此操作无法撤销。
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirmDialog = false">取消</Button>
          <Button variant="danger" :loading="isOperating" @click="confirmDelete">停止并删除</Button>
        </div>
      </template>
    </Modal>

    <!-- Global Loading Overlay -->
    <div v-if="isOperating" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div class="bg-white dark:bg-dark-800 rounded-lg p-6 flex flex-col items-center gap-4 min-w-[200px]">
        <Spinner size="lg" color="primary" />
        <p class="text-gray- dark:text-dark-300">{{ operatingMessage }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore, type SessionInfo } from '@/stores/session'
import Button from '@/components/common/Button.vue'
import Modal from '@/components/common/Modal.vue'
import SessionItem from '@/components/desktop/SessionItem.vue'
import Spinner from '@/components/common/Spinner.vue'
import { useToast } from '@/composables/useToast'
import { useSessionWindows } from '@/composables/useSessionWindows'
import { useSessionStatusListener } from '@/composables/useSessionStatusListener'

const router = useRouter()
const sessionStore = useSessionStore()
const toast = useToast()
const { closeTerminalWindow } = useSessionWindows()
const { startListening, stopListening } = useSessionStatusListener()

// 监听会话列表变化，自动关闭已停止会话的终端窗口
watch(() => sessionStore.sessions, (newSessions, oldSessions) => {
  if (!oldSessions) return

  for (const oldSession of oldSessions) {
    const newSession = newSessions.find(s => s.id === oldSession.id)

    // 如果会话从运行中变为停止/错误，关闭终端窗口
    if (oldSession.status === 'running' || oldSession.status === 'waitingInput') {
      if (newSession && (newSession.status === 'stopped' || newSession.status === 'error')) {
        closeTerminalWindow(oldSession.id)
      }
    }

    // 如果会话被删除
    if (!newSession) {
      closeTerminalWindow(oldSession.id)
    }
  }
}, { deep: true })

const isLoading = ref(true)
// 对话框状态
const showStopConfirmDialog = ref(false)
const showDeleteConfirmDialog = ref(false)
const pendingSession = ref<SessionInfo | null>(null)

// 操作中的 loading 状态
const isOperating = ref(false)
const operatingMessage = ref('处理中...')

// 所有会话（包括已停止的）
const allSessions = computed(() => {
  return sessionStore.sessions
})

onMounted(async () => {
  isLoading.value = true
  await sessionStore.loadSessions()
  isLoading.value = false

  // 启动会话状态变化监听
  await startListening()
})

onUnmounted(() => {
  stopListening()
})

async function refreshSessions() {
  await sessionStore.loadSessions()
  toast.info('会话列表已刷新')
}

function viewSession(session: SessionInfo) {
  // 检查会话是否在运行
  if (session.status !== 'running' && session.status !== 'waitingInput') {
    toast.info('会话未运行，无法查看终端')
    return
  }

  // 点击查看按钮时，打开独立终端窗口（已由 SessionItem 处理）
  // 此处保留仅用于 emit 事件
}

function confirmStopSession(session: SessionInfo) {
  pendingSession.value = session
  showStopConfirmDialog.value = true
}

async function confirmStop() {
  if (!pendingSession.value) return

  const sessionId = pendingSession.value.id
  isOperating.value = true
  operatingMessage.value = '正在停止会话...'

  try {
    await sessionStore.killSession(sessionId)
    toast.info('会话已停止')

    // 立即关闭终端窗口
    closeTerminalWindow(sessionId)
  } catch (e) {
    toast.error('停止会话失败: ' + (e as Error).message)
  } finally {
    isOperating.value = false
    showStopConfirmDialog.value = false
    pendingSession.value = null
  }
}

async function restartSession(session: SessionInfo) {
  isOperating.value = true
  operatingMessage.value = '正在重启会话...'

  try {
    const newSessionId = await sessionStore.restartSession(session.id)
    toast.success('会话已重启')

    // 自动选中新启动的会话
    const newSession = sessionStore.sessions.find(s => s.id === newSessionId)
    if (newSession) {
      // 重启后终端窗口会自动连接
    }
  } catch (e) {
    toast.error('重启会话失败: ' + (e as Error).message)
  } finally {
    isOperating.value = false
  }
}

function confirmDeleteSession(session: SessionInfo) {
  pendingSession.value = session

  // 如果会话还在运行，提示将先停止再删除
  if (session.status !== 'stopped' && session.status !== 'error') {
    showDeleteConfirmDialog.value = true
  } else {
    // 已停止的会话直接删除
    confirmDelete()
  }
}

async function confirmDelete() {
  if (!pendingSession.value) return

  const sessionId = pendingSession.value.id
  const isRunning = pendingSession.value.status !== 'stopped' && pendingSession.value.status !== 'error'

  isOperating.value = true
  operatingMessage.value = isRunning ? '正在停止并删除会话...' : '正在删除会话...'

  try {
    // 如果会话还在运行，先停止
    if (isRunning) {
      await sessionStore.killSession(sessionId)
    }
    // 然后删除
    await sessionStore.deleteSession(sessionId)
    toast.success('会话已删除')

    // 立即关闭终端窗口
    closeTerminalWindow(sessionId)
  } catch (e) {
    toast.error('删除会话失败: ' + (e as Error).message)
  } finally {
    isOperating.value = false
    showDeleteConfirmDialog.value = false
    pendingSession.value = null
  }
}

function goToSessionConfig() {
  router.push({ name: 'sessions' })
}
</script>