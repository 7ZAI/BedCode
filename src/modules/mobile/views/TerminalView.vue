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
        :rendered-index="renderedIndex"
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
import { createStreamingDecoder } from '@/modules/shared/composables/useTauri'

// 定义组件名称，用于 KeepAlive 缓存
defineOptions({
  name: 'MobileTerminal',
})

import { useRouter, useRoute } from 'vue-router'
import { useMobileConnection } from '@/modules/shared/composables/useMobileConnection'
import { wsLoadSessions, wsSendInput, wsResizeTerminal } from '@/modules/shared/composables/useMobileCommands'
import MobileTerminal from '@/modules/mobile/components/MobileTerminal.vue'
import InputAssistant from '@/modules/mobile/components/InputAssistant.vue'

const router = useRouter()
const route = useRoute()

// 注入屏幕方向
const isLandscape = inject<Ref<boolean>>('isLandscape', ref(false))
const isLandscapeValue = computed(() => isLandscape.value)

const connection = useMobileConnection()

// 使用统一的连接状态
const isConnectedValue = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 输出缓冲区
const outputBuffer = ref<string>('')
const MAX_OUTPUT_BUFFER = 512000 // 500KB 上限，防止内存泄漏

// 跟踪终端已渲染的输出索引，用于增量写入
// 由父组件完全控制，避免子组件索引不同步问题
const renderedIndex = ref(0)

/** 安全追加输出到缓冲区，超限时从头部裁剪 */
function appendOutput(data: string) {
  outputBuffer.value += data
  // 更新已渲染索引为当前缓冲区长度
  renderedIndex.value = outputBuffer.value.length
  if (outputBuffer.value.length > MAX_OUTPUT_BUFFER) {
    outputBuffer.value = outputBuffer.value.slice(-MAX_OUTPUT_BUFFER)
    // 缓冲区裁剪后，需要重置 renderedIndex
    renderedIndex.value = outputBuffer.value.length
  }
}

const terminalRef = ref<any>(null)

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
  renderedIndex.value = 0
  terminalRef.value?.clear()
}

// Terminal ready handler
function onTerminalReady() {
  console.log('Mobile terminal ready')
  // 终端准备好后，同步当前已渲染的索引
  renderedIndex.value = outputBuffer.value.length
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
  renderedIndex.value = 0
}

// KeepAlive 恢复时触发，重置渲染��引避免重复显示
function onTerminalActivated() {
  console.log('[TerminalView] onTerminalActivated, resetting renderedIndex')
  renderedIndex.value = 0
  // 同时清空终端显示
  terminalRef.value?.clear()
}

let unlistenOutput: UnlistenFn | null = null

// 流式解码器：PTY 输出每 4096 字节切一次，用 streaming 模式确保多字节
// UTF-8 字符被切分到两个 chunk 时仍能正确解码，而非输出 U+FFFD
const outputDecoder = createStreamingDecoder()

// 断线重连后清除旧输出
watch(() => connection.connectionStatus.value, (newStatus, oldStatus) => {
  if ((newStatus === 'connected' || newStatus === 'paired') &&
      (oldStatus === 'disconnected' || oldStatus === 'error' || oldStatus === undefined)) {
    console.log('[TerminalView] Reconnected, clearing output buffer')
    outputBuffer.value = ''
    renderedIndex.value = 0
    outputDecoder.flush()
    terminalRef.value?.clear()
  }
})

onMounted(async () => {
  // 加载会话列表以获取会话名称
  try {
    activeSessionsList.value = await wsLoadSessions()
  } catch (e) {
    console.error('[TerminalView] Failed to load sessions:', e)
  }

  // 监听 ws_output 事件，只显示当前活跃会话的输出
  unlistenOutput = await listen<{ session_id: string; data: string; is_waiting: boolean }>('ws_output', (event) => {
    // 只显示当前活跃会话的输出
    if (connection.activeSessionId.value && event.payload.session_id !== connection.activeSessionId.value) {
      return
    }
    const decoded = outputDecoder.decode(event.payload.data)
    appendOutput(decoded)
  })
})

onUnmounted(async () => {
  unlistenOutput?.()
})

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