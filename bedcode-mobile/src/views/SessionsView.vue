<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="flex-shrink-0 bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center justify-between gap-2">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ t('mobile.session.title') }}</h1>
      <!-- Mock Terminal Toggle (DEV only) -->
      <button
        v-if="mockTerminal.isDev"
        class="p-2 rounded-lg transition-colors"
        :class="mockTerminal.enabled.value ? 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-muted)] hover:bg-[var(--mobile-border)]'"
        @click="mockTerminal.toggle()"
        :title="t('mobile.session.mockToggle')"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
      </button>
      <button
        v-if="isConnected"
        class="p-2 rounded-lg hover:bg-[var(--mobile-border)] transition-colors"
        @click="refreshSessions"
        :title="t('mobile.session.refresh')"
      >
        <svg
          class="w-5 h-5 text-[var(--mobile-accent)]"
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
      <!-- Sessions list -->
      <div class="p-4 space-y-3">
        <!-- 未连接提示 -->
        <div v-if="!isConnected" class="text-center text-[var(--mobile-text-muted)] py-4 mb-4 border-b border-[var(--mobile-border)]">
          <p class="text-sm">{{ t('mobile.session.notConnected') }}</p>
        </div>

        <!-- Mock Terminal Session (DEV only) -->
        <SessionCard
          v-if="mockTerminal.isDev && mockTerminal.enabled.value"
          :session="mockSession"
          @click="handleMockSessionClick"
          @stop=""
          @delete=""
        />

        <!-- 真实会话列表 -->
        <template v-if="isConnected">
          <div v-if="realSessions.length === 0" class="text-center text-[var(--mobile-text-muted)] py-4">
            {{ t('mobile.session.noSessions') }}
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
    <div v-if="isConnected" class="px-4 py-2 bg-[var(--mobile-bg-secondary)] border-t border-[var(--mobile-border)]">
      <span class="text-[var(--mobile-text-muted)] text-xs font-medium">{{ t('mobile.session.sessionCount', { name: currentDeviceName, count: realSessions.length }) }}</span>
    </div>

    <!-- Loading Overlay: 跳转终端期间显示 -->
    <transition name="mobile-loading-fade">
      <div v-if="isNavigating" class="mobile-loading-overlay">
        <div class="mobile-loading-spinner"></div>
        <p class="mobile-loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" :title="t('mobile.session.confirmStop')" size="sm">
      <p class="text-[var(--mobile-text-secondary)]">
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
      <p class="text-[var(--mobile-text-secondary)]">
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
import { httpStopSession, httpRemoveSession } from '@/composables/useHttpApi'
import { useToast } from '@/composables/useToast'
import { useMockTerminal, MOCK_SESSION_ID } from '@/composables/useMockTerminal'
import SessionCard from '@/components/SessionCard.vue'
import Modal from '@/components/Modal.vue'
import Button from '@/components/Button.vue'

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()
const { t } = useI18n()

// 模拟终端（DEV）
const mockTerminal = useMockTerminal()

// 连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前设备名称
const currentDeviceName = computed(() => connection.currentDevice.value?.name || t('mobile.session.connected'))

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

function handleSessionClick(session: any) {
  if (isNavigating.value) return
  isNavigating.value = true
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