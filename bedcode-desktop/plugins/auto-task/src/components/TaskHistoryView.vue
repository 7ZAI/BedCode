<script setup lang="ts">
/**
 * 任务历史视图 — 显示任务执行历史和当前队列
 *
 * 通过 inject('pluginContext') 获取 PluginContext，
 * 调用 Rust 后端命令查询数据，监听事件实时更新
 */
import { ref, onMounted, onUnmounted, inject, computed } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

const context = inject<PluginContext>('pluginContext')!

// ==================== State ====================

interface TaskRecord {
  id: string
  name: string
  status: string
  session_id: string
  auto_approve: number
  exit_reason: string | null
  created_at: string
  started_at: string | null
  completed_at: string | null
}

interface QueueItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

const tasks = ref<TaskRecord[]>([])
const queue = ref<QueueItem[]>([])
const loading = ref(true)
const selectedSessionId = ref('')

// ==================== Computed ====================

const statusLabel: Record<string, string> = {
  idle: '空闲',
  in_progress: '执行中',
  asking: '等待输入',
  completed: '已完成',
  interrupted: '已中断',
  pending: '待执行',
}

const statusColor: Record<string, string> = {
  idle: 'text-[var(--text-tertiary)]',
  in_progress: 'text-blue-500',
  asking: 'text-amber-500',
  completed: 'text-green-500',
  interrupted: 'text-red-500',
  pending: 'text-[var(--text-tertiary)]',
}

const statusDot: Record<string, string> = {
  idle: 'bg-[var(--text-tertiary)]',
  in_progress: 'bg-blue-500',
  asking: 'bg-amber-500',
  completed: 'bg-green-500',
  interrupted: 'bg-red-500',
  pending: 'bg-[var(--text-tertiary)]',
}

const currentTask = computed(() =>
  tasks.value.find(t => t.status === 'in_progress' || t.status === 'asking')
)

const historyTasks = computed(() =>
  tasks.value.filter(t => t.status === 'completed' || t.status === 'interrupted')
)

// ==================== Data Loading ====================

async function loadHistory() {
  try {
    const result = await context.commands.execute('auto-task.list-task-history')
    if (result?.tasks) {
      tasks.value = result.tasks
    }
  } catch (e) {
    console.error('[Auto Task] Failed to load history:', e)
  }
}

async function loadQueue(sessionId: string) {
  if (!sessionId) {
    queue.value = []
    return
  }
  try {
    const result = await context.commands.execute('auto-task.list-task-queue', { session_id: sessionId })
    if (result?.tasks) {
      queue.value = result.tasks
    }
  } catch (e) {
    console.error('[Auto Task] Failed to load queue:', e)
  }
}

async function refresh() {
  loading.value = true
  await loadHistory()
  if (selectedSessionId.value) {
    await loadQueue(selectedSessionId.value)
  }
  loading.value = false
}

// ==================== Event Handlers ====================

function onStatusChanged() {
  loadHistory()
}

function onQueueChanged(data: any) {
  if (data?.session_id === selectedSessionId.value || !selectedSessionId.value) {
    loadQueue(data?.session_id || selectedSessionId.value)
  }
  loadHistory()
}

// ==================== Lifecycle ====================

let statusDisposable: any = null
let queueDisposable: any = null

onMounted(async () => {
  await refresh()

  statusDisposable = context.events.on('task:statusChanged', onStatusChanged)
  queueDisposable = context.events.on('task:queueChanged', onQueueChanged)
})

onUnmounted(() => {
  statusDisposable?.dispose()
  queueDisposable?.dispose()
})

// ==================== Helpers ====================

function formatTime(isoStr: string | null): string {
  if (!isoStr) return '-'
  try {
    const d = new Date(isoStr.replace(' ', 'T'))
    return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
  } catch {
    return isoStr
  }
}

function selectSession(sessionId: string) {
  selectedSessionId.value = sessionId
  loadQueue(sessionId)
}
</script>

<template>
  <div class="h-full flex flex-col bg-[var(--bg-primary)]">
    <!-- Header -->
    <div class="px-4 py-3 border-b border-[var(--border)] flex-shrink-0">
      <h2 class="text-sm font-semibold text-[var(--text-primary)]">任务历史</h2>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="flex-1 flex items-center justify-center">
      <span class="text-sm text-[var(--text-tertiary)]">加载中...</span>
    </div>

    <div v-else class="flex-1 overflow-y-auto px-4 py-3 space-y-4">
      <!-- Current Task -->
      <div v-if="currentTask" class="rounded-lg border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 p-3">
        <div class="flex items-center gap-2 mb-1">
          <div class="w-2 h-2 rounded-full" :class="statusDot[currentTask.status] || 'bg-blue-500'"></div>
          <span class="text-xs font-medium" :class="statusColor[currentTask.status] || 'text-blue-500'">
            {{ statusLabel[currentTask.status] || currentTask.status }}
          </span>
        </div>
        <p class="text-sm text-[var(--text-primary)] truncate">{{ currentTask.name || currentTask.session_id }}</p>
        <p class="text-xs text-[var(--text-tertiary)] mt-1">{{ formatTime(currentTask.started_at || currentTask.created_at) }}</p>
      </div>

      <!-- Task Queue -->
      <div v-if="queue.length > 0">
        <h3 class="text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2">
          待执行队列 ({{ queue.length }})
        </h3>
        <div class="space-y-1">
          <div
            v-for="item in queue"
            :key="item.id"
            class="flex items-center gap-2 px-3 py-2 rounded-md bg-[var(--bg-hover)] text-sm"
          >
            <span class="text-xs text-[var(--text-tertiary)] w-5 text-right flex-shrink-0">#{{ item.position }}</span>
            <span class="text-[var(--text-primary)] truncate flex-1">{{ item.prompt }}</span>
          </div>
        </div>
      </div>

      <!-- History -->
      <div v-if="historyTasks.length > 0">
        <h3 class="text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider mb-2">
          历史记录
        </h3>
        <div class="space-y-1">
          <div
            v-for="task in historyTasks"
            :key="task.id"
            class="flex items-center gap-2 px-3 py-2 rounded-md hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
            @click="selectSession(task.session_id)"
          >
            <div class="w-2 h-2 rounded-full flex-shrink-0" :class="statusDot[task.status] || 'bg-[var(--text-tertiary)]'"></div>
            <div class="flex-1 min-w-0">
              <p class="text-sm text-[var(--text-primary)] truncate">{{ task.name || task.session_id }}</p>
              <p class="text-xs text-[var(--text-tertiary)]">{{ formatTime(task.completed_at || task.created_at) }}</p>
            </div>
            <span class="text-xs flex-shrink-0" :class="statusColor[task.status]">
              {{ statusLabel[task.status] || task.status }}
            </span>
          </div>
        </div>
      </div>

      <!-- Empty State -->
      <div v-if="!currentTask && queue.length === 0 && historyTasks.length === 0" class="flex-1 flex flex-col items-center justify-center py-12">
        <svg class="w-12 h-12 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
        </svg>
        <p class="text-sm text-[var(--text-tertiary)]">暂无任务记录</p>
        <p class="text-xs text-[var(--text-tertiary)] mt-1">启动会话后任务将自动记录</p>
      </div>
    </div>
  </div>
</template>
