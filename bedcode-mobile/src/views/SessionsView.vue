<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="page-header-row">
        <div class="min-w-0 flex-1">
          <h1 class="page-title">{{ t('mobile.session.title') }}</h1>
        </div>
        <div class="flex items-center gap-2">
          <button
            v-if="mockTerminal.isDev"
            class="w-11 h-11 flex items-center justify-center rounded-lg transition-colors active:opacity-80"
            :class="mockTerminal.enabled.value ? 'chip-cyan' : ''"
            style="color: var(--mobile-text-muted)"
            @click="mockTerminal.toggle()"
            :title="t('mobile.session.mockToggle')"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
            </svg>
          </button>
          <button
            v-if="isConnected"
            class="w-11 h-11 flex items-center justify-center rounded-lg transition-colors active:opacity-80"
            style="color: var(--mobile-accent)"
            @click="refreshSessions"
            :title="t('mobile.session.refresh')"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-auto px-4 pb-8">
      <!-- 未连接提示（图标 + 引导） -->
      <div v-if="!isConnected" class="min-h-[45vh] flex flex-col items-center justify-center text-center">
        <svg class="w-12 h-12 mb-4" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
        <p class="group-row-sub">{{ t('mobile.session.notConnected') }}</p>
        <p class="text-sm mt-1" style="color: var(--mobile-text-disabled)">{{ t('mobile.session.notConnectedHint') }}</p>
        <button
          class="mt-5 h-11 px-6 rounded-xl text-sm font-medium transition-colors active:opacity-80"
          style="background: var(--mobile-accent); color: var(--mobile-text-on-accent)"
          @click="router.push({ name: 'mobile-home', query: { page: '0' } })"
        >
          {{ t('mobile.connection.scanConnect') }}
        </button>
      </div>

      <template v-else>
        <!-- Mock Terminal Session (DEV only) -->
        <div v-if="mockTerminal.isDev && mockTerminal.enabled.value" class="mb-4">
          <div class="group-card">
            <SessionCard
              :session="mockSession"
              @click="handleMockSessionClick"
              @stop=""
              @delete=""
            />
          </div>
        </div>

        <!-- 真实会话列表空态 -->
        <div v-if="realSessions.length === 0" class="min-h-[40vh] flex flex-col items-center justify-center text-center">
          <svg class="w-12 h-12 mb-4" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="group-row-sub">{{ t('mobile.session.noSessions') }}</p>
          <p class="text-sm mt-1" style="color: var(--mobile-text-disabled)">{{ t('mobile.session.noSessionsHint') }}</p>
        </div>

        <div v-else class="group-card">
          <SessionCard
            v-for="session in realSessions"
            :key="session.id"
            :session="session"
            @click="handleSessionClick(session)"
            @stop="handleStopSession(session)"
            @delete="handleDeleteSession(session)"
          />
        </div>
      </template>
    </div>

    <!-- Loading Dialog: 终端准备中（弹窗遮罩，会话页面保持可见；就绪后才跳转） -->
    <LoadingDialog :visible="isNavigating" :message="t('mobile.terminal.preparing')" />

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" :title="t('mobile.session.confirmStop')" size="sm">
      <p style="color: var(--mobile-text-secondary)">
        {{ t('mobile.session.confirmStopMsg', { name: pendingSession?.name || pendingSession?.id }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="isStopping" @click="confirmStop">{{ t('common.button.stop') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirmation Modal -->
    <Modal v-model="showDeleteConfirm" :title="t('mobile.session.confirmDelete')" size="sm">
      <p style="color: var(--mobile-text-secondary)">
        {{ t('mobile.session.confirmDeleteMsg', { name: pendingSession?.name || pendingSession?.id }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="isDeleting" @click="confirmDelete">{{ t('common.button.delete') }}</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onActivated } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import { httpStopSession, httpRemoveSession } from '@/composables/useHttpApi'
import { useToast } from '@/composables/useToast'
import { useMockTerminal, MOCK_SESSION_ID } from '@/composables/useMockTerminal'
import SessionCard from '@/components/SessionCard.vue'
import Modal from '@/components/Modal.vue'
import Button from '@/components/Button.vue'
import LoadingDialog from '@/components/LoadingDialog.vue'

const router = useRouter()
const connection = useMobileConnection()
const { prepareSession } = useTerminalBuffer()
const toast = useToast()
const { t } = useI18n()

// 模拟终端（DEV）
const mockTerminal = useMockTerminal()

// 连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

const mockSession = computed(() => ({
  id: MOCK_SESSION_ID,
  name: t('mobile.session.mockName'),
  config_id: '',
  status: 'running',
  created_at: new Date().toISOString(),
  is_active: true,
  sessionType: 'pty',
}))

// 真实会话列表（来自桌面端）
const realSessions = computed(() => connection.activeSessions.value)

// 停止确认弹窗
const showStopConfirm = ref(false)
const pendingSession = ref<any>(null)
const isStopping = ref(false)

// 删除确认弹窗
const showDeleteConfirm = ref(false)
const isDeleting = ref(false)

function handleMockSessionClick() {
  router.push({
    name: 'mobile-terminal',
    params: { id: MOCK_SESSION_ID },
  })
}

// 真实会话处理函数
const isNavigating = ref(false)

async function handleSessionClick(session: any) {
  if (isNavigating.value) return
  isNavigating.value = true
  connection.activeSessionId.value = session.id

  // 终端准备：订阅输出（回放帧缓冲在 store），就绪后才跳转 —— loading 以
  // 弹窗形式展示在本页，终端页挂载即渲染历史；失败/超时不阻塞跳转，由
  // 终端页走原有 forceReplay + 订阅重试路径
  if (session.status === 'running' || session.status === 'waiting_input') {
    await prepareSession(session.id)
  }

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
    const result = await httpStopSession(pendingSession.value.id)
    if (result.code !== 0) {
      toast.error(result.message || t('mobile.session.stopFailed'))
      return
    }
    // 立即更新本地状态（同步事件会排除操作者，所以需要手动更新）
    connection.stopSession(pendingSession.value.id)
    showStopConfirm.value = false
    pendingSession.value = null
  } catch (e) {
    console.error('[SessionsView] Failed to stop session:', e)
    toast.error(t('mobile.session.stopFailed'))
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
    const result = await httpRemoveSession(pendingSession.value.id)
    if (result.code !== 0) {
      toast.error(result.message || t('mobile.session.deleteFailed'))
      return
    }
    // 立即更新本地状态（同步事件会排除操作者，所以需要手动更新）
    connection.removeSession(pendingSession.value.id)
    showDeleteConfirm.value = false
    pendingSession.value = null
  } catch (e) {
    console.error('[SessionsView] Failed to delete session:', e)
    toast.error(t('mobile.session.deleteFailed'))
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
    toast.error(t('mobile.session.loadFailed'))
  }
}

onActivated(() => {
  // 从终端返回时重置导航状态
  isNavigating.value = false
})

onMounted(async () => {
  // 全局状态由同步事件自动维护，无需手动加载
})
</script>

<style scoped>
</style>