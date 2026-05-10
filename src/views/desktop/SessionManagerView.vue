<template>
  <div class="h-full flex">
    <!-- Left Panel: All Sessions List -->
    <div class="w-1/2 flex flex-col bg-dark-900 border-r border-dark-700">
      <!-- Header -->
      <header class="bg-dark-800 border-b border-dark-700 px-6 py-3 h-12 flex items-center">
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
          <p class="text-dark-400">加载中...</p>
        </div>

        <!-- Empty State -->
        <div v-else-if="allSessions.length === 0" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-dark-400">暂无会话</p>
          <p class="text-dark-500 text-sm mt-2">在"会话配置"页面启动会话</p>
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
            :show-terminal="expandedSessionId === session.id"
            @view="viewSession(session)"
            @stop="confirmStopSession(session)"
            @restart="restartSession(session)"
            @delete="confirmDeleteSession(session)"
          />
        </div>
      </div>
    </div>

    <!-- Right Panel: Terminal Preview -->
    <div class="w-1/2 flex flex-col">
      <TerminalPreview
        v-if="showTerminal && selectedSession"
        :session="selectedSession"
        :show-input="true"
      />
      <div v-else class="h-full flex flex-col items-center justify-center text-dark-500 bg-dark-900">
        <svg class="w-20 h-20 mb-4 text-dark-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
        </svg>
        <p class="text-lg">点击"查看"按钮查看终端</p>
        <p class="text-dark-600 text-sm mt-2">或点击左侧会话行查看终端输出</p>
      </div>
    </div>

    <!-- Stop Confirm Dialog -->
    <Modal v-model="showStopConfirmDialog" title="确认停止会话" size="sm">
      <p class="text-dark-300">确定要停止会话 "<span class="text-white font-medium">{{ pendingSession?.name }}</span>" 吗？</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirmDialog = false">取消</Button>
          <Button variant="danger" @click="confirmStop">停止</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirm Dialog -->
    <Modal v-model="showDeleteConfirmDialog" title="确认删除会话" size="sm">
      <p class="text-dark-300">
        会话 "<span class="text-white font-medium">{{ pendingSession?.name }}</span>" 仍在运行，将先停止再删除。此操作无法撤销。
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirmDialog = false">取消</Button>
          <Button variant="danger" @click="confirmDelete">停止并删除</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useSessionStore, type SessionInfo } from '@/stores/session'
import Button from '@/components/common/Button.vue'
import Modal from '@/components/common/Modal.vue'
import SessionItem from '@/components/desktop/SessionItem.vue'
import TerminalPreview from '@/components/desktop/TerminalPreview.vue'
import Spinner from '@/components/common/Spinner.vue'
import { useToast } from '@/composables/useToast'

const router = useRouter()
const sessionStore = useSessionStore()
const toast = useToast()

const isLoading = ref(true)
const expandedSessionId = ref<string | null>(null)
const selectedSession = ref<SessionInfo | null>(null)
const showTerminal = ref(false)

// 对话框状态
const showStopConfirmDialog = ref(false)
const showDeleteConfirmDialog = ref(false)
const pendingSession = ref<SessionInfo | null>(null)

// 所有会话（包括已停止的）
const allSessions = computed(() => {
  return sessionStore.sessions
})

onMounted(async () => {
  isLoading.value = true
  await sessionStore.loadSessions()
  isLoading.value = false
})

async function refreshSessions() {
  await sessionStore.loadSessions()
  toast.info('会话列表已刷新')
}

function viewSession(session: SessionInfo) {
  // 只有运行中的会话才能查看终端
  if (session.status !== 'stopped' && session.status !== 'error') {
    selectedSession.value = session
    showTerminal.value = true
  }
}

function confirmStopSession(session: SessionInfo) {
  pendingSession.value = session
  showStopConfirmDialog.value = true
}

async function confirmStop() {
  if (!pendingSession.value) return

  try {
    await sessionStore.killSession(pendingSession.value.id)
    toast.info('会话已停止')

    // 如果当前查看的是被停止的会话，清除选中
    if (selectedSession.value?.id === pendingSession.value.id) {
      showTerminal.value = false
    }
  } catch (e) {
    toast.error('停止会话失败: ' + (e as Error).message)
  } finally {
    showStopConfirmDialog.value = false
    pendingSession.value = null
  }
}

async function restartSession(session: SessionInfo) {
  try {
    const newSessionId = await sessionStore.restartSession(session.id)
    toast.success('会话已重启')

    // 自动选中新启动的会话
    const newSession = sessionStore.sessions.find(s => s.id === newSessionId)
    if (newSession) {
      selectedSession.value = newSession
      showTerminal.value = true
    }
  } catch (e) {
    toast.error('重启会话失败: ' + (e as Error).message)
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

  try {
    // 如果会话还在运行，先停止
    if (pendingSession.value.status !== 'stopped' && pendingSession.value.status !== 'error') {
      await sessionStore.killSession(pendingSession.value.id)
    }
    // 然后删除
    await sessionStore.deleteSession(pendingSession.value.id)
    toast.success('会话已删除')

    // 如果当前查看的是被删除的会话，清除选中
    if (selectedSession.value?.id === pendingSession.value.id) {
      selectedSession.value = null
      showTerminal.value = false
    }
  } catch (e) {
    toast.error('删除会话失败: ' + (e as Error).message)
  } finally {
    showDeleteConfirmDialog.value = false
    pendingSession.value = null
  }
}

function goToSessionConfig() {
  router.push({ name: 'sessions' })
}
</script>