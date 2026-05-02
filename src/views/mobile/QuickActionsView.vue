<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3">
      <h1 class="text-lg font-semibold">快捷指令</h1>
    </header>

    <!-- Quick Actions -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Preset Actions Grid -->
      <div class="mb-6">
        <h3 class="text-dark-400 text-sm font-medium mb-3">预设指令</h3>
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
          <h3 class="text-dark-400 text-sm font-medium">自定义指令</h3>
          <button
            class="text-primary-400 text-sm"
            @click="showAddDialog = true"
          >
            + 添加
          </button>
        </div>

        <div v-if="customActions.length === 0" class="text-center py-8">
          <p class="text-dark-500 text-sm">暂无自定义指令</p>
        </div>

        <div v-else class="space-y-2">
          <div
            v-for="action in customActions"
            :key="action.id"
            class="bg-dark-800 rounded-xl p-4 flex items-center gap-3 active:bg-dark-700"
          >
            <div
              class="w-10 h-10 rounded-lg flex items-center justify-center"
              :style="{ backgroundColor: (action.color || '#6b7280') + '20' }"
            >
              <span class="text-lg">{{ action.icon || '⚡' }}</span>
            </div>
            <div class="flex-1 min-w-0">
              <p class="font-medium truncate">{{ action.name }}</p>
              <p class="text-dark-400 text-sm truncate">{{ action.content }}</p>
            </div>
            <div class="flex gap-2">
              <button
                class="p-2 text-dark-400 hover:text-white"
                @click="editAction(action)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
              </button>
              <button
                class="p-2 text-dark-400 hover:text-red-400"
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
          <div class="absolute inset-0 bg-black/60" @click="closeDialog"></div>
          <div class="relative w-full max-w-sm bg-dark-800 rounded-2xl p-6">
            <h3 class="text-lg font-semibold mb-4">
              {{ editingAction ? '编辑指令' : '添加指令' }}
            </h3>

            <div class="space-y-4">
              <div>
                <label class="text-dark-400 text-sm mb-1 block">名称</label>
                <input
                  v-model="form.name"
                  type="text"
                  placeholder="指令名称"
                  class="w-full bg-dark-700 border border-dark-600 rounded-lg px-3 py-2 text-white placeholder-dark-400 focus:outline-none focus:border-primary-500"
                />
              </div>

              <div>
                <label class="text-dark-400 text-sm mb-1 block">内容</label>
                <textarea
                  v-model="form.content"
                  placeholder="指令内容"
                  rows="3"
                  class="w-full bg-dark-700 border border-dark-600 rounded-lg px-3 py-2 text-white placeholder-dark-400 focus:outline-none focus:border-primary-500 resize-none"
                ></textarea>
              </div>

              <div>
                <label class="text-dark-400 text-sm mb-1 block">图标</label>
                <div class="flex gap-2">
                  <button
                    v-for="emoji in iconOptions"
                    :key="emoji"
                    :class="[
                      'w-10 h-10 rounded-lg text-lg',
                      form.icon === emoji ? 'bg-primary-600' : 'bg-dark-700'
                    ]"
                    @click="form.icon = emoji"
                  >
                    {{ emoji }}
                  </button>
                </div>
              </div>

              <div>
                <label class="text-dark-400 text-sm mb-1 block">颜色</label>
                <div class="flex gap-2">
                  <button
                    v-for="color in colorOptions"
                    :key="color"
                    :class="[
                      'w-8 h-8 rounded-full',
                      form.color === color ? 'ring-2 ring-white ring-offset-2 ring-offset-dark-800' : ''
                    ]"
                    :style="{ backgroundColor: color }"
                    @click="form.color = color"
                  ></button>
                </div>
              </div>
            </div>

            <div class="flex gap-3 mt-6">
              <button
                class="flex-1 bg-dark-700 text-dark-300 py-2.5 rounded-xl font-medium"
                @click="closeDialog"
              >
                取消
              </button>
              <button
                class="flex-1 bg-primary-600 text-white py-2.5 rounded-xl font-medium"
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
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import QuickActionButton from '@/components/mobile/QuickActionButton.vue'
import { invoke } from '@tauri-apps/api/core'

interface QuickAction {
  id: string
  name: string
  content: string
  icon?: string
  color?: string
}

const router = useRouter()

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
    const actions = await invoke<QuickAction[]>('list_quick_actions')
    // Filter out preset actions (first 4)
    customActions.value = actions.slice(4)
  } catch (error) {
    console.error('Failed to load quick actions:', error)
  }
}

function sendQuickAction(action: QuickAction) {
  // Navigate to terminal and send action
  // In real app, this would send to active session
  router.push('/mobile/devices')

  // Emit event for terminal to pick up
  window.dispatchEvent(new CustomEvent('quick-action', {
    detail: action.content
  }))
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
