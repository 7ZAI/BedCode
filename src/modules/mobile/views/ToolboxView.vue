<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">工具箱</h1>
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

    <!-- Toolbox Sections -->
    <div class="flex-1 overflow-auto p-4 space-y-5">

      <!-- Section: 预设任务 -->
      <section>
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">预设任务</h3>
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

        <!-- 自定义指令 -->
        <div class="mt-4">
          <div class="flex items-center justify-between mb-3">
            <h4 class="text-[var(--mobile-text-muted)] text-sm font-medium">自定义指令</h4>
            <button
              class="text-[var(--mobile-accent)] text-sm hover:text-cyan-300 transition-colors"
              @click="showAddDialog = true"
            >
              + 添加
            </button>
          </div>

          <div v-if="customActions.length === 0" class="text-center py-4">
            <p class="text-[var(--mobile-text-disabled)] text-sm">暂无自定义指令</p>
          </div>

          <div v-else class="space-y-2">
            <div
              v-for="action in customActions"
              :key="action.id"
              class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-3 flex items-center gap-3 hover:border-cyan-500/30 transition-all"
            >
              <div
                class="w-9 h-9 rounded-lg flex items-center justify-center border"
                :style="{ backgroundColor: (action.color || '#6b7280') + '15', borderColor: (action.color || '#6b7280') + '30' }"
              >
                <span class="text-base">{{ action.icon || '⚡' }}</span>
              </div>
              <div class="flex-1 min-w-0">
                <p class="font-medium text-[var(--mobile-text-primary)] text-sm truncate">{{ action.name }}</p>
                <p class="text-[var(--mobile-text-muted)] text-xs truncate">{{ action.content }}</p>
              </div>
              <div class="flex gap-1">
                <button
                  class="p-1.5 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
                  @click="editAction(action)"
                >
                  <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                  </svg>
                </button>
                <button
                  class="p-1.5 text-[var(--mobile-text-muted)] hover:text-red-400 transition-colors"
                  @click="deleteAction(action.id)"
                >
                  <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- Section: 项目文件 -->
      <section>
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">项目文件</h3>
        <div
          v-if="!activeSessionId"
          class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 text-center"
        >
          <p class="text-[var(--mobile-text-disabled)] text-sm">连接设备后查看项目文件</p>
        </div>
        <div v-else class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl overflow-hidden">
          <!-- 文件树工具栏 -->
          <div class="flex items-center justify-between px-3 py-2 border-b border-[var(--mobile-border)]">
            <span class="text-xs text-[var(--mobile-text-muted)]">文件目录</span>
            <div class="flex gap-1">
              <button
                class="p-1 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
                title="刷新"
                @click="handleFileRefresh"
              >
                <svg class="w-3.5 h-3.5" :class="{ 'animate-spin': fileLoading }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              </button>
              <button
                class="p-1 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
                title="全部折叠"
                @click="fileCollapseAll"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </button>
              <button
                class="p-1 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
                title="全部展开"
                @click="fileExpandAll"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                </svg>
              </button>
            </div>
          </div>

          <!-- 文件树内容 -->
          <div class="max-h-64 overflow-y-auto p-1">
            <div v-if="fileLoading" class="flex items-center justify-center py-6">
              <svg class="w-4 h-4 animate-spin text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </div>
            <div v-else-if="fileError" class="text-center py-4">
              <p class="text-xs text-red-400">{{ fileError }}</p>
              <button class="text-xs text-[var(--mobile-accent)] mt-1" @click="handleFileRefresh">重试</button>
            </div>
            <div v-else-if="fileTree.length === 0" class="text-center py-4">
              <p class="text-xs text-[var(--mobile-text-disabled)]">暂无文件</p>
            </div>
            <template v-else>
              <FileTreeItem
                v-for="(node, index) in fileTree"
                :key="index"
                :node="node"
                :depth="0"
                @file-click="handleFileClick"
              />
            </template>
          </div>
        </div>
      </section>

      <!-- Section: 插件（预留） -->
      <section>
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">插件</h3>
        <div class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 text-center">
          <p class="text-[var(--mobile-text-disabled)] text-sm">即将推出</p>
        </div>
      </section>

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

    <!-- File Viewer Modal -->
    <FileViewerModal
      :visible="showFileViewer"
      :filename="selectedFile"
      @update:visible="showFileViewer = $event"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { wsSendInput } from '@/modules/mobile/composables/useMobileCommands'
import { useFileTree } from '@/modules/mobile/composables/useFileTree'
import QuickActionButton from '@/modules/mobile/components/QuickActionButton.vue'
import FileTreeItem from '@/modules/mobile/components/FileTreeItem.vue'
import FileViewerModal from '@/modules/mobile/components/FileViewerModal.vue'
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

const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')
const currentDeviceName = computed(() => connection.currentDevice.value?.name || '')
const activeSessionId = computed(() => connection.activeSessionId.value || '')

// ==================== 预设任务 ====================

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
    customActions.value = actions.slice(4)
  } catch (error) {
    console.error('Failed to load quick actions:', error)
  }
}

async function sendQuickAction(action: QuickAction) {
  const sessionId = connection.activeSessionId.value
  if (sessionId) {
    try {
      await wsSendInput(sessionId, action.content)
    } catch (e) {
      console.error('Failed to send quick action:', e)
      router.push('/mobile/devices')
    }
  } else {
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
    const index = customActions.value.findIndex(a => a.id === action.id)
    if (index >= 0) {
      customActions.value[index] = action
    }
  } else {
    customActions.value.push(action)
  }

  closeDialog()
}

// ==================== 项目文件 ====================

const sessionIdRef = computed(() => activeSessionId.value || '')
const { tree: fileTree, loading: fileLoading, error: fileError, expandAll: fileExpandAll, collapseAll: fileCollapseAll, refresh: fileRefresh } = useFileTree(sessionIdRef)

const showFileViewer = ref(false)
const selectedFile = ref('')

async function handleFileRefresh() {
  await fileRefresh()
}

function handleFileClick(name: string) {
  selectedFile.value = name
  showFileViewer.value = true
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
