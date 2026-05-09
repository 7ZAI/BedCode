<template>
  <div class="h-full flex">
    <!-- Left Panel: Session List -->
    <div class="w-1/2 flex flex-col bg-dark-900 border-r border-dark-700">
      <!-- Header -->
      <header class="bg-dark-800 border-b border-dark-700 px-6 py-3 h-12 flex items-center">
        <div class="flex items-center justify-between w-full">
          <h2 class="text-lg font-semibold">会话管理</h2>
          <Button variant="primary" @click="showCreateDialog = true">
            <template #icon>
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
              </svg>
            </template>
            新建会话
          </Button>
        </div>
      </header>

      <!-- Session List -->
      <div class="flex-1 overflow-auto p-6">
        <!-- Loading State -->
        <div v-if="isLoading" class="text-center py-12">
          <Spinner size="xl" color="primary" class="mb-4" />
          <p class="text-dark-400">加载中...</p>
        </div>

        <!-- Empty State -->
        <div v-else-if="sessionStore.configs.length === 0" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-dark-400">暂无会话配置</p>
          <p class="text-dark-500 text-sm mt-2">点击"新建会话"创建第一个会话</p>
        </div>

        <div v-else class="grid grid-cols-1 gap-4">
          <SessionCard
            v-for="config in sessionStore.configs"
            :key="config.id"
            :config="config"
            @start="startSession(config.id)"
            @edit="editConfig(config)"
            @delete="deleteConfig(config.id)"
          />
        </div>
      </div>

      <!-- Running Sessions -->
      <div v-if="runningSessions.length > 0" class="border-t border-dark-700 p-4 bg-dark-850">
        <h3 class="text-sm font-medium text-dark-400 mb-3">运行中的会话</h3>
        <div class="flex gap-3 overflow-x-auto pb-2">
          <div
            v-for="session in runningSessions"
            :key="session.id"
            :class="[
              'flex-shrink-0 bg-dark-700 rounded-lg px-4 py-2 flex items-center gap-3 cursor-pointer hover:bg-dark-600 transition-colors',
              sessionStore.activeSession?.id === session.id ? 'ring-2 ring-primary-500' : ''
            ]"
            @click="selectSession(session)"
          >
            <div
              :class="[
                'w-2 h-2 rounded-full',
                session.status === 'running' ? 'bg-green-500' :
                session.status === 'waitingInput' ? 'bg-yellow-500' :
                session.status === 'error' ? 'bg-red-500' : 'bg-dark-500'
              ]"
            ></div>
            <span class="text-sm">{{ session.name }}</span>
            <button
              @click.stop="killSession(session.id)"
              class="text-dark-400 hover:text-red-400 transition-colors"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Right Panel: Terminal Preview -->
    <div class="w-1/2 flex flex-col">
      <TerminalPreview
        v-if="sessionStore.activeSession"
        :session="sessionStore.activeSession"
        :show-input="true"
      />
      <div v-else class="h-full flex flex-col items-center justify-center text-dark-500 bg-dark-900">
        <svg class="w-20 h-20 mb-4 text-dark-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-lg">选择一个运行中的会话</p>
        <p class="text-dark-600 text-sm mt-2">点击下方运行中的会话查看终端输出</p>
      </div>
    </div>

    <!-- Create/Edit Dialog -->
    <Modal v-model="showCreateDialog" title="新建会话" size="lg">
      <SessionForm
        :config="editingConfig"
        @save="handleSaveConfig"
        @cancel="showCreateDialog = false"
      />
    </Modal>

    <!-- Delete Confirm Dialog -->
    <Modal v-model="showDeleteConfirmDialog" title="确认删除" size="sm">
      <p class="text-dark-300">确定要删除此会话配置吗？此操作无法撤销。</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDeleteConfirmDialog = false">取消</Button>
          <Button variant="danger" @click="confirmDelete">删除</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSessionStore, type SessionConfig, type SessionInfo } from '@/stores/session'
import Button from '@/components/common/Button.vue'
import Modal from '@/components/common/Modal.vue'
import SessionCard from '@/components/desktop/SessionCard.vue'
import SessionForm from '@/components/desktop/SessionForm.vue'
import TerminalPreview from '@/components/desktop/TerminalPreview.vue'
import Spinner from '@/components/common/Spinner.vue'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'
import { useToast } from '@/composables/useToast'

const sessionStore = useSessionStore()
const toast = useToast()

const showCreateDialog = ref(false)
const editingConfig = ref<SessionConfig | null>(null)
const isLoading = ref(true)
const showDeleteConfirmDialog = ref(false)
const pendingDeleteConfigId = ref<string | null>(null)

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

// 只显示运行中的会话（排除已停止的）
// 注意：后端使用 camelCase 序列化，SessionStatus::Stopped 变成 "stopped"
const runningSessions = computed(() => {
  return sessionStore.sessions.filter(s => s.status !== 'stopped')
})

onMounted(async () => {
  isLoading.value = true
  await sessionStore.loadConfigs()
  await sessionStore.loadSessions()
  isLoading.value = false

  // 等待 DOM 更新完成，确保首页渲染完毕
  await nextTick()

  // 输出应用启动耗时（从 Rust 进程启动到首页渲染完成的总耗时）
  try {
    const elapsed = await invoke<number>('get_startup_time')
    console.log(`[BedCode] 应用启动耗时: ${elapsed}ms (从进程启动到首页渲染完成)`)
  } catch (e) {
    // 非 Tauri 环境（如浏览器开发）忽略
  }
})

async function startSession(configId: string) {
  try {
    const sessionId = await sessionStore.createSession(configId)
    toast.success('会话已启动')

    // 自动选中新启动的会话
    const session = sessionStore.sessions.find(s => s.id === sessionId)
    if (session) {
      sessionStore.activeSession = session
    }
  } catch (e) {
    toast.error('启动会话失败: ' + (e as Error).message)
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
  try {
    console.log('Killing session:', sessionId)
    await sessionStore.killSession(sessionId)
    console.log('Session killed, sessions:', sessionStore.sessions)
    toast.info('会话已终止')
  } catch (e) {
    console.error('Failed to kill session:', e)
    toast.error('终止会话失败: ' + (e as Error).message)
  }
}

function selectSession(session: SessionInfo) {
  sessionStore.activeSession = session
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
      // 更新已有配置
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
      // 创建新配置
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