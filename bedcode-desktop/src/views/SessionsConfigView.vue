<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-page px-8 h-14 flex items-center border-b border-[var(--border)]">
      <div class="flex items-center justify-between w-full">
        <h2 class="text-[var(--font-size-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.sidebar.session') }}</h2>
        <button
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-all duration-200"
          @click="refreshSessions"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Config List -->
    <div class="flex-1 overflow-auto p-6 px-8">
      <!-- Loading State -->
      <div v-if="isLoading" class="text-center py-12 animate-fade-slide-up">
        <Spinner size="xl" color="primary" class="mb-4" />
        <p class="text-[var(--text-secondary)]">{{ t('common.status.loading') }}</p>
      </div>

      <!-- Empty State -->
      <div v-else-if="configs.length === 0" class="text-center py-12 animate-fade-slide-up">
        <svg class="w-16 h-16 mx-auto text-[var(--text-tertiary)] mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-[var(--text-primary)]">{{ t('desktop.session.noConfig') }}</p>
        <p class="text-[var(--text-secondary)] text-sm mt-2">{{ t('desktop.session.noConfigHint') }}</p>
        <Button variant="primary" class="mt-6" @click="showCreateDialog = true">
          <template #icon>
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
          </template>
          {{ t('desktop.session.newConfig') }}
        </Button>
      </div>

      <!-- Config Cards -->
      <div v-else class="space-y-4">
        <!-- New Config Entry Card -->
        <button
          @click="showCreateDialog = true"
          class="w-full h-14 rounded-card border-2 border-dashed border-[var(--border-input)] hover:border-brand bg-transparent hover:bg-[var(--color-primary-light)] text-[var(--text-tertiary)] hover:text-brand flex items-center justify-center gap-2 transition-all duration-200 group animate-fade-slide-up"
        >
          <svg class="w-5 h-5 transition-transform duration-200 group-hover:scale-110" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          <span class="text-sm font-medium">{{ t('desktop.session.newConfig') }}</span>
        </button>

        <SessionCard
          v-for="(config, index) in configs"
          :key="config.id"
          :config="config"
          :sessions="sessions"
          class="animate-fade-slide-up"
          :style="{ animationDelay: `${(index + 1) * 50}ms` }"
          @start="startSession(config.id)"
          @edit="editConfig(config)"
          @delete="deleteConfig(config.id)"
          @view-session="viewSession"
          @stop-session="confirmStopSession"
          @restart-session="restartSession"
          @delete-session="confirmDeleteSession"
        />
      </div>
    </div>

    <!-- Create/Edit Dialog -->
    <Modal v-model="showCreateDialog" :title="editingConfig ? t('desktop.session.editConfig') : t('desktop.session.newConfig')" size="lg">
      <SessionForm
        ref="sessionFormRef"
        :config="editingConfig"
        @save="handleSaveConfig"
      />
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="secondary" @click="showCreateDialog = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="primary" @click="submitForm">{{ editingConfig ? t('common.button.save') : t('common.button.create') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirm Dialog -->
    <Modal v-model="showDeleteConfirmDialog" :title="t('desktop.session.confirmDelete')" size="sm">
      <p class="text-[var(--text-primary)]">{{ t('desktop.session.confirmDeleteMsg') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirmDialog = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" @click="confirmDeleteConfig">{{ t('common.button.delete') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Stop Session Confirm Dialog -->
    <Modal v-model="showStopConfirmDialog" :title="t('desktop.session.confirmStop')" size="sm">
      <p class="text-[var(--text-primary)]">{{ t('desktop.session.confirmStopMsg', { name: pendingSession?.name }) }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirmDialog = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="isOperating" @click="confirmStop">{{ t('common.button.stop') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Session Confirm Dialog -->
    <Modal v-model="showDeleteSessionConfirmDialog" :title="t('desktop.session.confirmDeleteSession')" size="sm">
      <p class="text-[var(--text-primary)]">
        {{ t('desktop.session.confirmDeleteRunning', { name: pendingSession?.name }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteSessionConfirmDialog = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="isOperating" @click="confirmDeleteSessionNow">{{ t('desktop.session.stopAndDelete') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Global Loading Overlay -->
    <div v-if="isOperating" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div class="bg-card rounded-card p-6 flex flex-col items-center gap-4 min-w-[200px] shadow-card">
        <Spinner size="lg" color="primary" />
        <p class="text-[var(--text-primary)]">{{ operatingMessage }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import Button from '@/components/Button.vue'
import Modal from '@/components/Modal.vue'
import SessionCard from '@/components/SessionCard.vue'
import SessionForm from '@/components/SessionForm.vue'
import Spinner from '@/components/Spinner.vue'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'
import { useToast } from '@/composables/useToast'
import { InvokeTimeoutError } from '@/utils/invoke'
import { useSessionStore, type SessionInfo, type SessionConfig } from '@/stores/session'
import { useSessionWindows } from '@/composables/useSessionWindows'
import { useSessionStatusListener } from '@/composables/useSessionStatusListener'
import { initSessionCache, destroySessionCache } from '@/composables/useGlobalTerminal'

const sessionStore = useSessionStore()
const { t } = useI18n()
const toast = useToast()
const { closeTerminalWindow } = useSessionWindows()
const { startListening, stopListening } = useSessionStatusListener()

const configs = computed(() => sessionStore.configs)
const sessions = computed(() => sessionStore.sessions)

const showCreateDialog = ref(false)
const editingConfig = ref<SessionConfig | null>(null)
const isLoading = ref(true)
const showDeleteConfirmDialog = ref(false)
const pendingDeleteConfigId = ref<string | null>(null)
const sessionFormRef = ref<InstanceType<typeof SessionForm> | null>(null)

// 会话操作对话框状态
const showStopConfirmDialog = ref(false)
const showDeleteSessionConfirmDialog = ref(false)
const pendingSession = ref<SessionInfo | null>(null)

// 操作中的 loading 状态
const isOperating = ref(false)
const operatingMessage = ref(t('desktop.session.processing'))

// 监听会话列表变化，自动关闭已停止会话的终端窗口
watch(() => sessionStore.sessions, (newSessions, oldSessions) => {
  if (!oldSessions) return

  for (const oldSession of oldSessions) {
    const newSession = newSessions.find(s => s.id === oldSession.id)

    // 会话从运行中变为停止/错误，关闭终端窗口
    if (oldSession.status === 'running' || oldSession.status === 'waitingInput') {
      if (newSession && (newSession.status === 'stopped' || newSession.status === 'error')) {
        closeTerminalWindow(oldSession.id)
      }
    }

    // 会话被删除
    if (!newSession) {
      closeTerminalWindow(oldSession.id)
    }
  }
}, { deep: true })

// Page-level keyboard shortcuts
useKeyboardShortcuts([
  { key: 'n', ctrl: true, handler: () => { showCreateDialog.value = true } },
  {
    key: 'Escape',
    handler: () => {
      showCreateDialog.value = false
      showDeleteConfirmDialog.value = false
      showStopConfirmDialog.value = false
      showDeleteSessionConfirmDialog.value = false
    },
    ignoreInput: true,
  },
])

onMounted(async () => {
  isLoading.value = true
  try {
    await sessionStore.loadConfigs()
    await sessionStore.loadSessions()
  } catch (e) {
    console.error('Failed to load data:', e)
  }
  isLoading.value = false

  // 启动会话状态变化监听
  await startListening()

  // 等待 DOM 更新完成
  await nextTick()

  // 输出应用启动耗时
  try {
    const elapsed = await invoke<number>('get_startup_time')
    console.log(`[BedCode] 应用启动耗时: ${elapsed}ms`)
  } catch (e) {
    // 非 Tauri 环境忽略
  }
})

onUnmounted(() => {
  stopListening()
})

async function refreshSessions() {
  await sessionStore.loadSessions()
  toast.info(t('desktop.session.listRefreshed'))
}

async function startSession(configId: string) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.starting')

  try {
    // 两阶段启动：
    // 1. 创建会话（不启动 PTY）
    const sessionId = await sessionStore.createSession(configId)

    // 2. 初始化会话历史缓存（用于存储终端输出）
    initSessionCache(sessionId)

    // 3. 启动 PTY
    await sessionStore.startSession(sessionId)

    toast.success(t('desktop.session.sessionStarted'))
    // 卡片折叠区域已通过会话列表变化自动展开
  } catch (e: any) {
    console.error('[SessionsConfigView] startSession error:', e)
    if (e instanceof InvokeTimeoutError) {
      toast.error(t('desktop.session.startTimeout'))
    } else {
      toast.error(t('desktop.session.startFailed', { error: e?.message || e }))
    }
  } finally {
    isOperating.value = false
  }
}

function editConfig(config: SessionConfig) {
  editingConfig.value = config
  showCreateDialog.value = true
}

function deleteConfig(configId: string) {
  pendingDeleteConfigId.value = configId
  showDeleteConfirmDialog.value = true
}

async function confirmDeleteConfig() {
  if (!pendingDeleteConfigId.value) return
  await sessionStore.deleteConfig(pendingDeleteConfigId.value)
  toast.success(t('desktop.session.configDeleted'))
  showDeleteConfirmDialog.value = false
  pendingDeleteConfigId.value = null
}

function viewSession(session: SessionInfo) {
  // 检查会话是否在运行
  if (session.status !== 'running' && session.status !== 'waitingInput') {
    toast.info(t('desktop.session.notRunning'))
    return
  }
  // 打开独立终端窗口已由 SessionItem 处理
}

function confirmStopSession(session: SessionInfo) {
  pendingSession.value = session
  showStopConfirmDialog.value = true
}

async function confirmStop() {
  if (!pendingSession.value) return

  const sessionId = pendingSession.value.id
  isOperating.value = true
  operatingMessage.value = t('desktop.session.stopping')

  try {
    await sessionStore.killSession(sessionId)
    // 销毁会话历史缓存
    destroySessionCache(sessionId)
    toast.info(t('desktop.session.sessionStopped'))

    // 立即关闭终端窗口
    closeTerminalWindow(sessionId)
  } catch (e) {
    toast.error(t('desktop.session.stopFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
    showStopConfirmDialog.value = false
    pendingSession.value = null
  }
}

async function restartSession(session: SessionInfo) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.restarting')

  try {
    await sessionStore.restartSession(session.id)
    toast.success(t('desktop.session.sessionRestarted'))
  } catch (e) {
    toast.error(t('desktop.session.restartFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
  }
}

function confirmDeleteSession(session: SessionInfo) {
  pendingSession.value = session

  // 运行中的会话提示将先停止再删除，已停止的会话直接删除
  if (session.status !== 'stopped' && session.status !== 'error') {
    showDeleteSessionConfirmDialog.value = true
  } else {
    confirmDeleteSessionNow()
  }
}

async function confirmDeleteSessionNow() {
  if (!pendingSession.value) return

  const sessionId = pendingSession.value.id
  const isRunning = pendingSession.value.status !== 'stopped' && pendingSession.value.status !== 'error'

  isOperating.value = true
  operatingMessage.value = isRunning ? t('desktop.session.stoppingAndDeleting') : t('desktop.session.deleting')

  try {
    // 运行中的会话先停止
    if (isRunning) {
      await sessionStore.killSession(sessionId)
      // 销毁会话历史缓存
      destroySessionCache(sessionId)
    }
    // 然后删除
    await sessionStore.deleteSession(sessionId)
    toast.success(t('desktop.session.sessionDeleted'))

    // 立即关闭终端窗口
    closeTerminalWindow(sessionId)
  } catch (e) {
    toast.error(t('desktop.session.deleteFailed', { error: (e as Error).message }))
  } finally {
    isOperating.value = false
    showDeleteSessionConfirmDialog.value = false
    pendingSession.value = null
  }
}

function submitForm() {
  if (sessionFormRef.value) {
    handleSaveConfig(sessionFormRef.value.form)
  }
}

interface SessionFormData {
  name: string
  environment: string
  wslDistro: string
  workingDir: string
  command: string
  autoStart: boolean
}

async function handleSaveConfig(form: SessionFormData) {
  try {
    if (editingConfig.value) {
      await sessionStore.updateConfig(
        editingConfig.value.id,
        form.name,
        form.environment,
        form.workingDir || '',
        form.command || '',
        form.wslDistro || undefined,
        form.autoStart,
      )
      toast.success(t('desktop.session.configUpdated'))
    } else {
      await sessionStore.createConfig(
        form.name,
        form.environment,
        form.workingDir || '',
        form.command || '',
        form.wslDistro || undefined,
      )
      toast.success(t('desktop.session.configCreated'))
    }
    showCreateDialog.value = false
    editingConfig.value = null
  } catch (e: any) {
    console.error('[SessionsConfigView] handleSaveConfig error:', e)
    toast.error(t('desktop.session.saveFailed', { error: e?.message || e }))
  }
}
</script>
