<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-page px-8 h-14 flex items-center border-b border-[var(--border)]">
      <h2 class="text-[var(--font-size-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.sidebar.sessionConfig') }}</h2>
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
          @view-session="goToSessionManager"
          @stop-session="killSession"
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
          <Button variant="danger" @click="confirmDelete">{{ t('common.button.delete') }}</Button>
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
import { ref, onMounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import Button from '@/components/Button.vue'
import Modal from '@/components/Modal.vue'
import SessionCard from '@/components/SessionCard.vue'
import SessionForm from '@/components/SessionForm.vue'
import Spinner from '@/components/Spinner.vue'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'
import { useToast } from '@/composables/useToast'
import { InvokeTimeoutError } from '@/utils/invoke'
import { initSessionCache, destroySessionCache } from '@/composables/useGlobalTerminal'
import {
  createSessionConfig,
  listSessionConfigs,
  deleteSessionConfig,
  updateSessionConfig,
  listSessions,
  createSessionNoStart,
  startExistingSession,
  killSession as stopSession,
  type SessionConfig
} from '@/composables/useDesktopCommands'

const router = useRouter()
const { t } = useI18n()
const toast = useToast()

const configs = ref<SessionConfig[]>([])
const sessions = ref<any[]>([])
const activeSession = ref<any | null>(null)
const showCreateDialog = ref(false)
const editingConfig = ref<SessionConfig | null>(null)
const isLoading = ref(true)
const showDeleteConfirmDialog = ref(false)
const pendingDeleteConfigId = ref<string | null>(null)
const sessionFormRef = ref<InstanceType<typeof SessionForm> | null>(null)

// 操作中的 loading 状态
const isOperating = ref(false)
const operatingMessage = ref(t('desktop.session.processing'))

// Page-level keyboard shortcuts
useKeyboardShortcuts([
  { key: 'n', ctrl: true, handler: () => { showCreateDialog.value = true } },
  {
    key: 'Escape',
    handler: () => {
      showCreateDialog.value = false
      showDeleteConfirmDialog.value = false
    },
    ignoreInput: true,
  },
])

onMounted(async () => {
  isLoading.value = true
  try {
    configs.value = await listSessionConfigs()
    sessions.value = await listSessions()
  } catch (e) {
    console.error('Failed to load data:', e)
  }
  isLoading.value = false

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

async function startSession(configId: string) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.starting')

  try {
    // 两阶段启动：
    // 1. 创建会话（不启动 PTY）
    const sessionId = await createSessionNoStart(configId)

    // 2. 初始化会话历史缓存（用于存储终端输出）
    initSessionCache(sessionId)

    // 3. 启动 PTY
    await startExistingSession(sessionId)

    // 刷新会话列表
    sessions.value = await listSessions()

    toast.success(t('desktop.session.sessionStarted'))
    // 跳转到会话管理页面
    router.push({ name: 'session-manager' })
  } catch (e: any) {
    console.error('[SessionsView] startSession error:', e)
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

async function deleteConfig(configId: string) {
  pendingDeleteConfigId.value = configId
  showDeleteConfirmDialog.value = true
}

async function confirmDelete() {
  if (!pendingDeleteConfigId.value) return
  await deleteSessionConfig(pendingDeleteConfigId.value)
  configs.value = await listSessionConfigs()
  toast.success(t('desktop.session.configDeleted'))
  showDeleteConfirmDialog.value = false
  pendingDeleteConfigId.value = null
}

async function killSession(sessionId: string) {
  isOperating.value = true
  operatingMessage.value = t('desktop.session.stopping')

  try {
    await stopSession(sessionId)
    // 销毁会话历史缓存
    destroySessionCache(sessionId)
    sessions.value = await listSessions()
    toast.info(t('desktop.session.sessionTerminated'))
  } catch (e: any) {
    console.error('[SessionsView] killSession error:', e)
    toast.error(t('desktop.session.terminateFailed', { error: e?.message || e }))
  } finally {
    isOperating.value = false
  }
}

function goToSessionManager() {
  router.push({ name: 'session-manager' })
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
  console.log('[SessionsView] handleSaveConfig called:', form)
  try {
    if (editingConfig.value) {
      console.log('[SessionsView] editing mode, calling updateSessionConfig')
      await updateSessionConfig({
        id: editingConfig.value.id,
        name: form.name,
        environment: form.environment,
        working_dir: form.workingDir || '',
        command: form.command || '',
        wsl_distro: form.wslDistro || undefined,
        auto_start: form.autoStart,
      })
      toast.success(t('desktop.session.configUpdated'))
    } else {
      console.log('[SessionsView] create mode, calling createSessionConfig')
      await createSessionConfig({
        name: form.name,
        environment: form.environment,
        working_dir: form.workingDir || '',
        command: form.command || '',
        wsl_distro: form.wslDistro || undefined,
      })
      toast.success(t('desktop.session.configCreated'))
    }
    configs.value = await listSessionConfigs()
    showCreateDialog.value = false
    editingConfig.value = null
  } catch (e: any) {
    console.error('[SessionsView] handleSaveConfig error:', e)
    console.error('[SessionsView] error message:', e?.message)
    toast.error(t('desktop.session.saveFailed', { error: e?.message || e }))
  }
}
</script>