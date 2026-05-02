<template>
  <div class="h-full flex flex-col">
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
      <div v-if="sessionStore.configs.length === 0" class="text-center py-12">
        <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-dark-400">暂无会话配置</p>
        <p class="text-dark-500 text-sm mt-2">点击"新建会话"创建第一个会话</p>
      </div>

      <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
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
          class="flex-shrink-0 bg-dark-700 rounded-lg px-4 py-2 flex items-center gap-3 cursor-pointer hover:bg-dark-600 transition-colors"
          @click="selectSession(session)"
        >
          <div
            :class="[
              'w-2 h-2 rounded-full',
              session.status === 'Running' ? 'bg-green-500' :
              session.status === 'WaitingInput' ? 'bg-yellow-500' :
              session.status === 'Error' ? 'bg-red-500' : 'bg-dark-500'
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

    <!-- Create/Edit Dialog -->
    <Modal v-model="showCreateDialog" title="新建会话" size="lg">
      <SessionForm
        :config="editingConfig"
        @save="handleSaveConfig"
        @cancel="showCreateDialog = false"
      />
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useSessionStore, type SessionConfig, type SessionInfo } from '@/stores/session'
import Button from '@/components/common/Button.vue'
import Modal from '@/components/common/Modal.vue'
import SessionCard from '@/components/desktop/SessionCard.vue'
import SessionForm from '@/components/desktop/SessionForm.vue'
import { useToast } from '@/composables/useToast'

const sessionStore = useSessionStore()
const toast = useToast()

const showCreateDialog = ref(false)
const editingConfig = ref<SessionConfig | null>(null)

// 只显示运行中的会话（排除已停止的）
const runningSessions = computed(() => {
  return sessionStore.sessions.filter(s => s.status !== 'Stopped')
})

onMounted(async () => {
  await sessionStore.loadConfigs()
})

async function startSession(configId: string) {
  try {
    await sessionStore.createSession(configId)
    toast.success('会话已启动')
  } catch (e) {
    toast.error('启动会话失败: ' + (e as Error).message)
  }
}

function editConfig(config: SessionConfig) {
  editingConfig.value = config
  showCreateDialog.value = true
}

async function deleteConfig(configId: string) {
  if (confirm('确定要删除此会话配置吗？')) {
    await sessionStore.deleteConfig(configId)
    toast.success('会话配置已删除')
  }
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
