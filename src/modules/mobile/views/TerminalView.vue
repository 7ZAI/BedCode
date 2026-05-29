<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header - 横屏时更紧凑 -->
    <header
      class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-2 flex items-center gap-2 shrink-0"
      :class="{ 'py-1': isLandscapeValue, 'py-3 pb-3': !isLandscapeValue }"
      :style="{ paddingTop: isLandscapeValue ? '8px' : '12px' }"
    >
      <button @click="goBack" class="p-2 -ml-2">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="flex-1 min-w-0">
        <h1 class="font-semibold truncate" :class="isLandscapeValue ? 'text-sm' : ''">{{ sessionName }}</h1>
      </div>
      <div class="flex items-center gap-1.5 text-xs">
        <div
          :class="[
            'w-1.5 h-1.5 rounded-full',
            isConnectedValue ? 'bg-green-500' : 'bg-red-500'
          ]"
        ></div>
        <span class="text-gray- dark:text-dark-400">{{ isConnectedValue ? '已连接' : '未连接' }}</span>
      </div>
      <button
        class="p-2 rounded-lg bg-gray-100 dark:bg-dark-700 text-gray- dark:text-dark-300"
        @click="handleClear"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output (使用 flex 填充剩余空间) -->
    <div class="flex-1 overflow-hidden min-h-0">
      <MobileTerminal
        ref="terminalRef"
        :output="outputBuffer"
        @ready="onTerminalReady"
        @clear="onTerminalClear"
        @resize="handleTerminalResize"
        @activated="onTerminalActivated"
      />
    </div>

    <!-- Input Assistant 悬浮球 -->
    <InputAssistant
      :terminal-ref="terminalRef"
      :terminal-instance="terminalInstance"
      :is-connected="isConnectedValue"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, inject, watch, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { useRouter, useRoute } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { wsLoadSessions, wsSendInput, wsResizeTerminal, wsJoinSession, wsGetTerminalHistory, wsLeaveSession } from '@/modules/mobile/composables/useMobileCommands'
import MobileTerminal from '@/modules/mobile/components/MobileTerminal.vue'
import InputAssistant from '@/modules/mobile/components/InputAssistant.vue'

// 定义组件名称，用于 KeepAlive 缓存
defineOptions({
  name: 'TerminalView',
})

// ws_output 事件的 payload 类型
interface WsOutputPayload {
  session_id: string
  data: string
  is_waiting: boolean
  index: number
}

const router = useRouter()
const route = useRoute()

// 注入屏幕方向
const isLandscape = inject<Ref<boolean>>('isLandscape', ref(false))
const isLandscapeValue = computed(() => isLandscape.value)

const connection = useMobileConnection()

// 使用统一的连接状态
const isConnectedValue = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 输出缓冲区（无大小限制，由 xterm.js scrollback 控制）
const outputBuffer = ref<string>('')

// 跟踪已渲染的全局索引，用于去重
const renderedIndex = ref(0)

// 已知的索引集合（用于快速去重）
const knownIndices = new Set<number>()

// 加载历史数据并订阅实时输出
async function loadHistoryAndSubscribe(sessionId: string) {
  try {
    // 1. 获取历史数据
    const history = await wsGetTerminalHistory(sessionId)
    console.log('[TerminalView] History loaded:', history.events.length, 'events, current_index:', history.current_index)

    // 2. 按索引排序历史事件（确保按顺序追加）
    const sortedEvents = [...history.events].sort((a, b) => a.index - b.index)

    // 3. 追加历史数据到缓冲区
    for (const event of sortedEvents) {
      if (!knownIndices.has(event.index)) {
        outputBuffer.value += event.data
        knownIndices.add(event.index)
      }
    }

    // 4. 更新已渲染索引
    renderedIndex.value = history.current_index
    console.log('[TerminalView] History applied, renderedIndex:', renderedIndex.value, 'known indices:', knownIndices.size)

    // 5. 订阅会话以接收实时输出
    await wsJoinSession(sessionId)
    console.log('[TerminalView] Subscribed to session for real-time output')
  } catch (e) {
    console.error('[TerminalView] Failed to load history:', e)
    // 即使加载历史失败，也尝试订阅实时输出
    await wsJoinSession(sessionId)
  }
}

// 追加输出到缓冲区（带索引去重）
function appendOutput(data: string, index: number) {
  // 检查是否已存在该索引的数据（避免重复）
  if (knownIndices.has(index)) {
    console.log('[TerminalView] Skipping duplicate output, index:', index)
    return
  }

  outputBuffer.value += data
  knownIndices.add(index)
  renderedIndex.value = index
  console.log('[TerminalView] Appended output, index:', index, 'total rendered:', renderedIndex.value)
}

const terminalRef = ref<InstanceType<typeof MobileTerminal> | null>(null)

// 终端操作接口（供 InputAssistant 使用）
const terminalInstance = computed(() => ({
  sendInput: async (data: string) => {
    const sessionId = connection.activeSessionId.value
    console.log('[TerminalView] sendInput sessionId=' + sessionId + ', data_len=' + data.length + ' data="' + data.slice(0, 100) + '"')
    if (!sessionId) { console.warn('[TerminalView] sendInput: no activeSessionId'); return }
    try {
      await wsSendInput(sessionId, data)
      console.log('[TerminalView] sendInput OK')
    } catch (e) {
      console.error('[TerminalView] sendInput failed:', e)
    }
  },
  sendInputWithEnter: async (data: string) => {
    const sessionId = connection.activeSessionId.value
    console.log('[TerminalView] sendInputWithEnter sessionId=' + sessionId + ', data_len=' + data.length + ' data="' + data.slice(0, 100) + '"')
    if (!sessionId) { console.warn('[TerminalView] sendInputWithEnter: no activeSessionId'); return }
    try {
      // 一次 invoke 同时发送文本和 Enter，避免两次独立 invoke 的竞态条件
      // 桌面端 Input handler 会先写入 data，再写入 special_key
      await wsSendInput(sessionId, data, 'enter')
      console.log('[TerminalView] sendInputWithEnter OK')
    } catch (e) {
      console.error('[TerminalView] sendInputWithEnter failed:', e)
    }
  },
  sendSpecialKey: async (key: string) => {
    const sessionId = connection.activeSessionId.value
    console.log('[TerminalView] sendSpecialKey sessionId=' + sessionId + ', key=' + key)
    if (!sessionId) { console.warn('[TerminalView] sendSpecialKey: no activeSessionId'); return }
    try {
      await wsSendInput(sessionId, '', key)
      console.log('[TerminalView] sendSpecialKey OK')
    } catch (e) {
      console.error('[TerminalView] sendSpecialKey failed:', e)
    }
  },
}))

// 活跃会话列表（用于获取会话名称）
const activeSessionsList = ref<any[]>([])

// 会话名称 - 从活跃会话中查找
const sessionName = computed(() => {
  const activeId = connection.activeSessionId.value
  if (activeId) {
    const found = activeSessionsList.value.find((s: any) => s.id === activeId)
    if (found?.name) return found.name
  }
  return connection.currentDevice.value?.name || '终端'
})

// 清空终端
function handleClear() {
  outputBuffer.value = ''
  terminalRef.value?.clear()
}

// Terminal ready handler
function onTerminalReady() {
  console.log('[MobileTerminal] ready')

  // 订阅会话以开始接收输出
  const sessionId = connection.activeSessionId.value
  if (sessionId) {
    wsJoinSession(sessionId).catch(e => console.error('[TerminalView] wsJoinSession failed:', e))
  }
}

// Terminal resize handler - 将移动端真实终端尺寸同步到桌面 PTY
// 确保 Claude Code 的输出按移动端屏幕宽度排版，避免 \r 光标定位错乱
function handleTerminalResize(cols: number, rows: number) {
  const sessionId = connection.activeSessionId.value
  if (sessionId && cols > 0 && rows > 0) {
    wsResizeTerminal(sessionId, cols, rows)
  }
}

// Terminal clear handler
function onTerminalClear() {
  outputBuffer.value = ''
}

// KeepAlive 恢复时触发，确保终端实例正确显示
// 不清空缓冲区，保持现有数据继续接收新输出
function onTerminalActivated() {
  console.log('[TerminalView] onTerminalActivated, keeping buffer and continuing to receive output')
  // xterm.js 实例由 KeepAlive 保持，无需重新初始化
  // 输出缓冲区保持不变，新数据会继续追加
  // 订阅也保持不变，继续接收实时输出
}

let unlistenOutput: UnlistenFn | null = null

// 监听 ws_output 事件，只显示当前活跃会话的输出
// 注意：后端已经做了 Base64 解码，前端直接接收解码后的字符串
watch(() => connection.connectionStatus.value, (newStatus, oldStatus) => {
  if ((newStatus === 'connected' || newStatus === 'paired') &&
      (oldStatus === 'disconnected' || oldStatus === 'error' || oldStatus === undefined)) {
    console.log('[TerminalView] Reconnected, clearing output buffer and reloading history')
    // 清空缓冲区和索引状态
    outputBuffer.value = ''
    knownIndices.clear()
    renderedIndex.value = 0
    terminalRef.value?.clear()

    // 重新加载历史数据并订阅实时输出
    const sessionId = connection.activeSessionId.value
    if (sessionId) {
      loadHistoryAndSubscribe(sessionId).catch(e => console.error('[TerminalView] Failed to reload history:', e))
    }
  }
})

onMounted(async () => {
  // 从路由获取会话 ID
  const sessionId = route.params.sessionId as string
  if (sessionId) {
    connection.activeSessionId.value = sessionId
  }

  // 加载会话列表以获取会话名称
  try {
    activeSessionsList.value = await wsLoadSessions()
  } catch (e) {
    console.error('[TerminalView] Failed to load sessions:', e)
  }

  // 监听 ws_output 事件，只显示当前活跃会话的输出
  // 事件 payload 现在包含 index 字段用于去重
  unlistenOutput = await listen<WsOutputPayload>('ws_output', (event) => {
    // 只显示当前活跃会话的输出
    if (connection.activeSessionId.value && event.payload.session_id !== connection.activeSessionId.value) {
      return
    }
    // 后端已解码，使用索引去重
    appendOutput(event.payload.data, event.payload.index)
  })

  // 如果已连接，加载历史数据
  if (isConnectedValue.value && sessionId) {
    await loadHistoryAndSubscribe(sessionId)
  }
})

// 注意：不使用 onUnmounted 取消订阅
// KeepAlive 缓存的组件在离开页面时不会真正 unmount
// 应保持订阅继续接收输出，返回时数据仍在缓冲区中

function goBack() {
  router.push('/mobile/sessions')
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