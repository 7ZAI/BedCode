<template>
  <div class="h-full flex flex-col bg-[#0a0a0f]">
    <!-- Header -->
    <header class="bg-[#12121a]/90 backdrop-blur-xl border-b border-cyan-500/10 px-4 pb-3 flex items-center justify-between" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold text-white tracking-wide">会话</h1>
      <button
        v-if="isConnected"
        class="p-2 rounded-lg hover:bg-cyan-500/10 transition-colors"
        @click="refreshSessions"
        title="刷新会话"
      >
        <svg
          class="w-5 h-5 text-cyan-400"
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
      <!-- Sessions list - 调试会话始终显示 -->
      <div class="p-4 space-y-3">
        <!-- 未连接提示 -->
        <div v-if="!isConnected" class="text-center text-gray-500 py-4 mb-4 border-b border-gray-800">
          <p class="text-sm">未连接到桌面端，真实会话不可用</p>
        </div>

        <!-- 调试会话（始终显示） -->
        <SessionCard
          :session="mockDebugSession"
          @click="handleDebugSessionClick"
          @stop="handleDebugSessionStop"
          @delete="handleDebugSessionDelete"
        />

        <!-- 真实会话列表（仅连接时显示） -->
        <template v-if="isConnected">
          <div v-if="realSessions.length === 0" class="text-center text-gray-500 py-4">
            暂无其他运行中的会话
          </div>

          <SessionCard
            v-for="session in realSessions"
            :key="session.id"
            :session="session"
            @click="handleSessionClick(session)"
            @stop="handleStopSession(session)"
            @delete="handleDeleteSession(session)"
          />
        </template>
      </div>
    </div>

    <!-- Connection info -->
    <div v-if="isConnected" class="px-4 py-2 bg-[#12121a] border-t border-cyan-500/10">
      <span class="text-gray-500 text-xs font-medium">{{ currentDeviceName }} · {{ realSessions.length }} 个会话</span>
    </div>

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" title="确认停止会话" size="sm">
      <p class="text-gray-400">
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
      <p class="text-gray-400">
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

// 调试会话（始终显示）
const mockDebugSession = {
  id: 'mock-debug-session',
  name: '调试会话 (模拟)',
  status: 'running' as const,
  created_at: new Date().toISOString(),
  pty_type: 'bash',
  config_id: 'mock-config',
  is_active: true,
}

// 真实会话列表（来自桌面端）
const realSessions = computed(() => connection.activeSessions.value)

// 停止确认弹窗
const showStopConfirm = ref(false)
const pendingSession = ref<any>(null)
const isStopping = ref(false)

// 删除确认弹窗
const showDeleteConfirm = ref(false)
const isDeleting = ref(false)

// 调试会话专用处理函数
function handleDebugSessionClick() {
  connection.activeSessionId.value = mockDebugSession.id
  router.push({
    name: 'mobile-terminal',
    params: { id: mockDebugSession.id },
  })
}

function handleDebugSessionStop() {
  // 调试会话不可停止，不做任何操作
}

function handleDebugSessionDelete() {
  // 调试会话不可删除，不做任何操作
}

// 真实会话处理函数
function handleSessionClick(session: any) {
  connection.activeSessionId.value = session.id
  router.push({
    name: 'mobile-terminal',
    params: { id: session.id },
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
    // 立即更新本地状态（同步事件会排除操作者，所以需要手动更新）
    connection.stopSession(pendingSession.value.id)
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
    // 立即更新本地状态（同步事件会排除操作者，所以需要手动更新）
    connection.removeSession(pendingSession.value.id)
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