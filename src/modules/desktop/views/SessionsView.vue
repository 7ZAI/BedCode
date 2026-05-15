<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header -->
    <header class="px-6 py-3 h-12 flex items-center border-b border-gray-200 dark:border-dark-700 bg-white dark:bg-dark-800">
      <div class="flex items-center justify-between w-full">
        <h2 class="text-lg font-semibold text-gray-900 dark:text-white">会话配置</h2>
        <Button variant="primary" @click="showCreateDialog = true">
          <template #icon>
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
          </template>
          新建配置
        </Button>
      </div>
    </header>

    <!-- Config List -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Loading State -->
      <div v-if="isLoading" class="text-center py-12">
        <Spinner size="xl" color="primary" class="mb-4" />
        <p class="text-gray-500 dark:text-dark-400">加载中...</p>
      </div>

      <!-- Empty State -->
      <div v-else-if="sessionStore.configs.length === 0" class="text-center py-12">
        <svg class="w-16 h-16 mx-auto text-gray-400 dark:text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-gray-600 dark:text-dark-400">暂无会话配置</p>
        <p class="text-gray-500 dark:text-dark-500 text-sm mt-2">点击"新建配置"创建第一个配置</p>
      </div>

      <!-- Config Cards (Long Card Mode) -->
      <div v-else class="space-y-3">
        <SessionCard
          v-for="config in sessionStore.configs"
          :key="config.id"
          :config="config"
          :sessions="sessionStore.sessions"
          @start="startSession(config.id)"
          @edit="editConfig(config)"
          @delete="deleteConfig(config.id)"
          @view-session="goToSessionManager"
          @stop-session="killSession"
        />
      </div>
    </div>

    <!-- Create/Edit Dialog -->
    <Modal v-model="showCreateDialog" :title="editingConfig ? '编辑配置' : '新建配置'" size="lg">
      <SessionForm
        ref="sessionFormRef"
        :config="editingConfig"
        @save="handleSaveConfig"
      />
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="secondary" @click="showCreateDialog = false">取消</Button>
          <Button variant="primary" @click="submitForm">{{ editingConfig ? '保存' : '创建' }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Delete Confirm Dialog -->
    <Modal v-model="showDeleteConfirmDialog" title="确认删除" size="sm">
      <p class="text-gray-700 dark:text-dark-300">确定要删除此会话配置吗？此操作无法撤销。</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirmDialog = false">取消</Button>
          <Button variant="danger" @click="confirmDelete">删除</Button>
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
import { ref, onMounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { useSessionStore, type SessionConfig } from '@/modules/shared/stores/session'
import Button from '@/modules/shared/components/Button.vue'
import Modal from '@/modules/shared/components/Modal.vue'
import SessionCard from '@/modules/desktop/components/SessionCard.vue'
import SessionForm from '@/modules/desktop/components/SessionForm.vue'
import Spinner from '@/modules/shared/components/Spinner.vue'
import { useKeyboardShortcuts } from '@/modules/shared/composables/useKeyboardShortcuts'
import { useToast } from '@/modules/shared/composables/useToast'

const router = useRouter()
const sessionStore = useSessionStore()
const toast = useToast()

const showCreateDialog = ref(false)
const editingConfig = ref<SessionConfig | null>(null)
const isLoading = ref(true)
const showDeleteConfirmDialog = ref(false)
const pendingDeleteConfigId = ref<string | null>(null)
const sessionFormRef = ref<InstanceType<typeof SessionForm> | null>(null)

// 操作中的 loading 状态
const isOperating = ref(false)
const operatingMessage = ref('处理中...')

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
  await sessionStore.loadConfigs()
  await sessionStore.loadSessions()
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
  operatingMessage.value = '正在启动会话...'

  try {
    await sessionStore.createSession(configId)
    toast.success('会话已启动')
    // 跳转到会话管理页面
    router.push({ name: 'session-manager' })
  } catch (e) {
    toast.error('启动会话失败: ' + (e as Error).message)
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
  await sessionStore.deleteConfig(pendingDeleteConfigId.value)
  toast.success('会话配置已删除')
  showDeleteConfirmDialog.value = false
  pendingDeleteConfigId.value = null
}

async function killSession(sessionId: string) {
  isOperating.value = true
  operatingMessage.value = '正在停止会话...'

  try {
    await sessionStore.killSession(sessionId)
    toast.info('会话已终止')
  } catch (e) {
    console.error('Failed to kill session:', e)
    toast.error('终止会话失败: ' + (e as Error).message)
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
  tmuxSession: string
  autoStart: boolean
}

async function handleSaveConfig(form: SessionFormData) {
  try {
    if (editingConfig.value) {
      await sessionStore.updateConfig(
        editingConfig.value.id,
        form.name,
        form.environment,
        form.workingDir,
        form.command,
        form.wslDistro || undefined,
        form.tmuxSession || undefined,
        form.autoStart
      )
      toast.success('会话配置已更新')
    } else {
      await sessionStore.createConfig(
        form.name,
        form.environment,
        form.workingDir,
        form.command,
        form.wslDistro || undefined,
        form.tmuxSession || undefined
      )
      toast.success('会话配置已创建')
    }
    showCreateDialog.value = false
    editingConfig.value = null
    await sessionStore.loadConfigs()
  } catch (e) {
    toast.error('保存失败: ' + (e as Error).message)
  }
}
</script>