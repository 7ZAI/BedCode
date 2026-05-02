<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3">
      <div class="flex items-center justify-between">
        <h1 class="text-lg font-semibold">历史记录</h1>
        <button
          v-if="history.length > 0"
          class="text-red-400 text-sm"
          @click="showClearConfirm = true"
        >
          清空
        </button>
      </div>

      <!-- Search -->
      <div class="mt-3 relative">
        <svg class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <input
          v-model="searchQuery"
          type="text"
          placeholder="搜索历史记录..."
          class="w-full bg-dark-700 border border-dark-600 rounded-lg pl-10 pr-4 py-2 text-white placeholder-dark-400 focus:outline-none focus:border-primary-500"
        />
      </div>
    </header>

    <!-- History List -->
    <div class="flex-1 overflow-auto">
      <div v-if="filteredHistory.length === 0" class="flex flex-col items-center justify-center h-full">
        <div class="w-16 h-16 rounded-full bg-dark-800 flex items-center justify-center mb-4">
          <svg class="w-8 h-8 text-dark-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </div>
        <p class="text-dark-500">
          {{ searchQuery ? '未找到匹配记录' : '暂无历史记录' }}
        </p>
      </div>

      <div v-else>
        <div
          v-for="(group, date) in groupedHistory"
          :key="date"
          class="border-b border-dark-800 last:border-0"
        >
          <!-- Date Header -->
          <div class="px-4 py-2 bg-dark-850 sticky top-0">
            <span class="text-dark-400 text-sm">{{ formatDate(date) }}</span>
          </div>

          <!-- Records -->
          <div class="divide-y divide-dark-800">
            <div
              v-for="record in group"
              :key="record.id"
              class="px-4 py-3 active:bg-dark-800"
              @click="viewRecord(record)"
            >
              <div class="flex items-start gap-3">
                <!-- Icon -->
                <div
                  :class="[
                    'w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0',
                    record.type === 'input' ? 'bg-blue-900/50' : 'bg-green-900/50'
                  ]"
                >
                  <svg
                    :class="['w-4 h-4', record.type === 'input' ? 'text-blue-400' : 'text-green-400']"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path v-if="record.type === 'input'" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                  </svg>
                </div>

                <!-- Content -->
                <div class="flex-1 min-w-0">
                  <p class="text-sm line-clamp-2">{{ record.content }}</p>
                  <p class="text-dark-500 text-xs mt-1">
                    {{ record.sessionName }} · {{ formatTime(record.timestamp) }}
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Clear Confirm Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showClearConfirm" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/60" @click="showClearConfirm = false"></div>
          <div class="relative w-full max-w-sm bg-dark-800 rounded-2xl p-6">
            <h3 class="text-lg font-semibold mb-2">清空历史记录</h3>
            <p class="text-dark-400 text-sm mb-4">
              确定要清空所有历史记录吗？此操作不可撤销。
            </p>
            <div class="flex gap-3">
              <button
                class="flex-1 bg-dark-700 text-dark-300 py-2.5 rounded-xl font-medium"
                @click="showClearConfirm = false"
              >
                取消
              </button>
              <button
                class="flex-1 bg-red-600 text-white py-2.5 rounded-xl font-medium"
                @click="clearHistory"
              >
                清空
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Record Detail Dialog -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showDetail && selectedRecord" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/60" @click="closeDetail"></div>
          <div class="relative w-full max-w-sm bg-dark-800 rounded-2xl flex flex-col max-h-[80vh]">
            <!-- Header -->
            <div class="px-4 py-3 border-b border-dark-700 flex items-center justify-between">
              <div>
                <p class="font-medium">{{ selectedRecord.sessionName }}</p>
                <p class="text-dark-400 text-xs">{{ formatDateTime(selectedRecord.timestamp) }}</p>
              </div>
              <button @click="closeDetail" class="p-2 text-dark-400">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            <!-- Content -->
            <div class="flex-1 overflow-auto p-4">
              <pre class="text-sm whitespace-pre-wrap">{{ selectedRecord.content }}</pre>
            </div>

            <!-- Actions -->
            <div class="p-4 border-t border-dark-700">
              <button
                v-if="selectedRecord.type === 'input'"
                class="w-full bg-primary-600 text-white py-3 rounded-xl font-medium"
                @click="resendInput(selectedRecord)"
              >
                重新发送
              </button>
              <button
                class="w-full bg-dark-700 text-dark-300 py-3 rounded-xl font-medium mt-2"
                @click="copyContent(selectedRecord.content)"
              >
                复制内容
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

interface HistoryRecord {
  id: string
  type: 'input' | 'output'
  content: string
  sessionName: string
  timestamp: number
}

const searchQuery = ref('')
const showClearConfirm = ref(false)
const showDetail = ref(false)
const selectedRecord = ref<HistoryRecord | null>(null)

// Placeholder history data
const history = ref<HistoryRecord[]>([
  {
    id: '1',
    type: 'input',
    content: '请帮我创建一个新的 Vue 组件',
    sessionName: 'Claude Code - Project',
    timestamp: Date.now() - 1000 * 60 * 5
  },
  {
    id: '2',
    type: 'output',
    content: '好的，我来帮你创建一个新的 Vue 组件。首先，让我了解一下你需要什么类型的组件...',
    sessionName: 'Claude Code - Project',
    timestamp: Date.now() - 1000 * 60 * 4
  },
  {
    id: '3',
    type: 'input',
    content: '请继续',
    sessionName: 'Claude Code - Project',
    timestamp: Date.now() - 1000 * 60 * 30
  },
  {
    id: '4',
    type: 'input',
    content: '修复登录页面的 Bug',
    sessionName: 'Claude Code - Another',
    timestamp: Date.now() - 1000 * 60 * 60 * 2
  },
])

const filteredHistory = computed(() => {
  if (!searchQuery.value) return history.value

  const query = searchQuery.value.toLowerCase()
  return history.value.filter(r =>
    r.content.toLowerCase().includes(query) ||
    r.sessionName.toLowerCase().includes(query)
  )
})

const groupedHistory = computed(() => {
  const groups: Record<string, HistoryRecord[]> = {}

  for (const record of filteredHistory.value) {
    const date = new Date(record.timestamp).toDateString()
    if (!groups[date]) {
      groups[date] = []
    }
    groups[date].push(record)
  }

  return groups
})

function formatDate(dateStr: string): string {
  const date = new Date(dateStr)
  const today = new Date()
  const yesterday = new Date(today)
  yesterday.setDate(yesterday.getDate() - 1)

  if (date.toDateString() === today.toDateString()) {
    return '今天'
  } else if (date.toDateString() === yesterday.toDateString()) {
    return '昨天'
  } else {
    return date.toLocaleDateString('zh-CN', { month: 'long', day: 'numeric' })
  }
}

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit'
  })
}

function formatDateTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })
}

function viewRecord(record: HistoryRecord) {
  selectedRecord.value = record
  showDetail.value = true
}

function closeDetail() {
  showDetail.value = false
  selectedRecord.value = null
}

function clearHistory() {
  history.value = []
  showClearConfirm.value = false
}

function resendInput(record: HistoryRecord) {
  // Emit event for terminal to pick up
  window.dispatchEvent(new CustomEvent('quick-action', {
    detail: record.content
  }))
  closeDetail()
}

function copyContent(content: string) {
  navigator.clipboard.writeText(content)
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

.line-clamp-2 {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
