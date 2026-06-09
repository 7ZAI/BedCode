<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">快捷指令</h1>
    </header>

    <!-- Connection Status -->
    <div v-if="isConnected" class="px-4 py-2 bg-[var(--mobile-success-muted)] border-b border-emerald-500/20 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-emerald-500 shadow-[0_0_6px_rgba(16,185,129,0.5)]"></div>
        <span class="text-[var(--mobile-success)] text-sm">已连接 {{ currentDeviceName }}</span>
      </div>
      <button
        class="text-xs text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
        @click="router.push('/mobile/devices')"
      >
        管理
      </button>
    </div>
    <div v-else class="px-4 py-2 bg-[var(--mobile-bg-secondary)] border-b border-[var(--mobile-border)] flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-gray-600"></div>
        <span class="text-[var(--mobile-text-muted)] text-sm">未连接</span>
      </div>
      <button
        class="text-xs text-[var(--mobile-accent)] hover:text-cyan-300 transition-colors"
        @click="router.push('/mobile/devices')"
      >
        连接
      </button>
    </div>

    <!-- Quick Actions -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Preset Actions Grid -->
      <div class="mb-6">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">预设指令</h3>
        <div class="grid grid-cols-2 gap-3">
          <QuickActionButton
            v-for="action in presetActions"
            :key="action.id"
            :name="action.name"
            :content="action.content"
            :icon="action.icon"
            :color="action.color"
            @click="sendQuickAction(action)"
          />
        </div>
      </div>

      <!-- Custom Actions -->
      <div>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium tracking-wider uppercase">自定义指令</h3>
          <button
            class="text-[var(--mobile-accent)] text-sm hover:text-cyan-300 transition-colors"
            @click="showAddDialog = true"
          >
            + 添加
          </button>
        </div>

        <div v-if="customActions.length === 0" class="text-center py-8">
          <p class="text-[var(--mobile-text-disabled)] text-sm">暂无自定义指令</p>
        </div>

        <div v-else class="space-y-2">
          <div
            v-for="action in customActions"
            :key="action.id"
            class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 flex items-center gap-3 hover:border-cyan-500/30 transition-all"
          >
            <div
              class="w-10 h-10 rounded-lg flex items-center justify-center border"
              :style="{ backgroundColor: (action.color || '#6b7280') + '15', borderColor: (action.color || '#6b7280') + '30' }"
            >
              <span class="text-lg">{{ action.icon || '⚡' }}</span>
            </div>
            <div class="flex-1 min-w-0">
              <p class="font-medium text-[var(--mobile-text-primary)] truncate">{{ action.name }}</p>
              <p class="text-[var(--mobile-text-muted)] text-sm truncate">{{ action.content }}</p>
            </div>
            <div class="flex gap-2">
              <button
                class="p-2 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
                @click="editAction(action)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
              </button>
              <button
                class="p-2 text-[var(--mobile-text-muted)] hover:text-red-400 transition-colors"
                @click="deleteAction(action.id)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Add/Edit Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showAddDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/80" @click="closeDialog"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border-hover)] rounded-2xl p-6">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4">
              {{ editingAction ? '编辑指令' : '添加指令' }}
            </h3>

            <div class="space-y-4">
              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">名称</label>
                <input
                  v-model="form.name"
                  type="text"
                  placeholder="指令名称"
                  class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-cyan-500/50 transition-colors"
                />
              </div>

              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">内容</label>
                <textarea
                  v-model="form.content"
                  placeholder="指令内容"
                  rows="3"
                  class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] rounded-lg px-3 py-2 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-cyan-500/50 transition-colors resize-none"
                ></textarea>
              </div>

              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">图标</label>
                <div class="flex gap-2">
                  <button
                    v-for="emoji in iconOptions"
                    :key="emoji"
                    :class="[
                      'w-10 h-10 rounded-lg text-lg border transition-colors',
                      form.icon === emoji ? 'bg-cyan-500/20 border-cyan-500/50' : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)] hover:border-cyan-500/30'
                    ]"
                    @click="form.icon = emoji"
                  >
                    {{ emoji }}
                  </button>
                </div>
              </div>

              <div>
                <label class="text-[var(--mobile-text-muted)] text-sm mb-1 block">颜色</label>
                <div class="flex gap-2">
                  <button
                    v-for="color in colorOptions"
                    :key="color"
                    :class="[
                      'w-8 h-8 rounded-full border-2 transition-all',
                      form.color === color ? 'border-white scale-110' : 'border-transparent hover:scale-105'
                    ]"
                    :style="{ backgroundColor: color }"
                    @click="form.color = color"
                  ></button>
                </div>
              </div>
            </div>

            <div class="flex gap-3 mt-6">
              <button
                class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] py-2.5 rounded-xl font-medium hover:border-cyan-500/40 transition-colors"
                @click="closeDialog"
              >
                取消
              </button>
              <button
                class="flex-1 bg-cyan-500/20 border border-cyan-500/30 text-[var(--mobile-accent)] py-2.5 rounded-xl font-medium hover:bg-cyan-500/30 transition-colors"
                :class="{ 'opacity-50': !form.name || !form.content }"
                :disabled="!form.name || !form.content"
                @click="saveAction"
              >
                保存
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { wsSendInput } from '@/modules/mobile/composables/useMobileCommands'
import QuickActionButton from '@/modules/mobile/components/QuickActionButton.vue'
import { invoke } from '@tauri-apps/api/core'

interface QuickAction {
  id: string
  name: string
  content: string
  icon?: string
  color?: string
}

const router = useRouter()
const connection = useMobileConnection()

// 使用统一的连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前设备名称
const currentDeviceName = computed(() => connection.currentDevice.value?.name || '')

const presetActions = ref<QuickAction[]>([
  { id: '1', name: '继续', content: '请继续', icon: '▶️', color: '#22c55e' },
  { id: '2', name: '解释代码', content: '请解释这段代码的作用', icon: '📝', color: '#3b82f6' },
  { id: '3', name: '修复 Bug', content: '请帮我修复这个 Bug', icon: '🔧', color: '#a855f7' },
  { id: '4', name: '提交代码', content: '请帮我提交代码', icon: '📤', color: '#f97316' },
])

const customActions = ref<QuickAction[]>([])
const showAddDialog = ref(false)
const editingAction = ref<QuickAction | null>(null)

const form = ref({
  name: '',
  content: '',
  icon: '⚡',
  color: '#3b82f6'
})

const iconOptions = ['⚡', '📝', '🔧', '📤', '🎯', '💡', '🚀', '⭐']
const colorOptions = ['#3b82f6', '#22c55e', '#a855f7', '#f97316', '#ef4444', '#ec4899']

onMounted(async () => {
  await loadQuickActions()
})

async function loadQuickActions() {
  try {
    const actions = await invoke<QuickAction[]>('list_quick_actions_mobile')
    // Filter out preset actions (first 4)
    customActions.value = actions.slice(4)
  } catch (error) {
    console.error('Failed to load quick actions:', error)
  }
}

async function sendQuickAction(action: QuickAction) {
  // 通过 WebSocket 直接发送到当前活跃会话
  const sessionId = connection.activeSessionId.value
  if (sessionId) {
    try {
      await wsSendInput(sessionId, action.content)
      console.log('Quick action sent:', action.name)
    } catch (e) {
      console.error('Failed to send quick action:', e)
      router.push('/mobile/devices')
    }
  } else {
    // 无活跃会话，跳转到设备页
    router.push('/mobile/devices')
  }
}

function editAction(action: QuickAction) {
  editingAction.value = action
  form.value = {
    name: action.name,
    content: action.content,
    icon: action.icon || '⚡',
    color: action.color || '#3b82f6'
  }
  showAddDialog.value = true
}

async function deleteAction(id: string) {
  customActions.value = customActions.value.filter(a => a.id !== id)
  // In real app, call backend to delete
}

function closeDialog() {
  showAddDialog.value = false
  editingAction.value = null
  form.value = { name: '', content: '', icon: '⚡', color: '#3b82f6' }
}

async function saveAction() {
  if (!form.value.name || !form.value.content) return

  const action: QuickAction = {
    id: editingAction.value?.id || Date.now().toString(),
    name: form.value.name,
    content: form.value.content,
    icon: form.value.icon,
    color: form.value.color
  }

  if (editingAction.value) {
    // Update existing
    const index = customActions.value.findIndex(a => a.id === action.id)
    if (index >= 0) {
      customActions.value[index] = action
    }
  } else {
    // Add new
    customActions.value.push(action)
  }

  // In real app, save to backend
  closeDialog()
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
