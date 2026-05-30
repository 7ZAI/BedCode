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
        <span class="text-gray-500 dark:text-dark-400">{{ isConnectedValue ? '已连接' : '未连接' }}</span>
      </div>
      <button
        class="p-2 rounded-lg bg-gray-100 dark:bg-dark-700 text-gray-500 dark:text-dark-300"
        @click="handleClear"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output (使用 flex 填充剩余空间) -->
    <div class="flex-1 overflow-hidden min-h-0 pb-[140px]">
      <MobileTerminal
        ref="terminalRef"
        :external-instance="externalTerminal"
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

    <!-- 底部输入栏 -->
    <TerminalInputBar
      :is-connected="isConnectedValue"
      :disabled="!isConnectedValue"
      :is-landscape="isLandscapeValue"
      @submit="onSubmit"
      @execute="onExecute"
      @special-key="onSpecialKey"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, inject, watch, type Ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import {
  getTerminal,
  hasTerminal,
  createHiddenTerminal,
  clearTerminal as globalClearTerminal,
} from '@/modules/mobile/composables/useGlobalTerminal'
import { wsSendInput, wsResizeTerminal, wsJoinSession } from '@/modules/mobile/composables/useMobileCommands'
import MobileTerminal from '@/modules/mobile/components/MobileTerminal.vue'
import InputAssistant from '@/modules/mobile/components/InputAssistant.vue'
import TerminalInputBar from '@/modules/mobile/components/TerminalInputBar.vue'

// 定义组件名称，用于 KeepAlive 缓存
defineOptions({
  name: 'TerminalView',
})

const router = useRouter()
const route = useRoute()

// 注入屏幕方向
const isLandscape = inject<Ref<boolean>>('isLandscape', ref(false))
const isLandscapeValue = computed(() => isLandscape.value)

const connection = useMobileConnection()

// 使用统一的连接状态
const isConnectedValue = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

const terminalRef = ref<InstanceType<typeof MobileTerminal> | null>(null)

// 终端实例（从全局管理器获取）
const externalTerminal = computed(() => {
  const sessionId = connection.activeSessionId.value
  if (!sessionId) return null
  return getTerminal(sessionId)
})

// 终端操作接口（供 InputAssistant 使用）
const terminalInstance = computed(() => {
  const sessionId = connection.activeSessionId.value
  console.log('[TerminalView] terminalInstance computed, sessionId=', sessionId)

  return {
    instance: externalTerminal.value,
    sessionId, // 暴露 sessionId 供调试
    sendInput: async (data: string) => {
      console.log('[TerminalView] sendInput called, sessionId=', sessionId, 'data=', data)
      if (!sessionId) {
        console.warn('[TerminalView] No active session, skip sendInput')
        throw new Error('No active session')
      }
      try {
        console.log('[TerminalView] Calling wsSendInput...')
        await wsSendInput(sessionId, data)
        console.log('[TerminalView] wsSendInput completed')
      } catch (e) {
        console.error('[TerminalView] sendInput failed:', e)
        throw e
      }
    },
    sendInputWithEnter: async (data: string) => {
      console.log('[TerminalView] sendInputWithEnter called, sessionId=', sessionId, 'data=', data)
      if (!sessionId) {
        console.warn('[TerminalView] No active session, skip sendInputWithEnter')
        throw new Error('No active session')
      }
      try {
        console.log('[TerminalView] Calling wsSendInput with enter...')
        await wsSendInput(sessionId, data, 'enter')
        console.log('[TerminalView] wsSendInput with enter completed')
      } catch (e) {
        console.error('[TerminalView] sendInputWithEnter failed:', e)
        throw e
      }
    },
    sendSpecialKey: async (key: string) => {
      console.log('[TerminalView] sendSpecialKey called, sessionId=', sessionId, 'key=', key)
      if (!sessionId) {
        console.warn('[TerminalView] No active session, skip sendSpecialKey')
        throw new Error('No active session')
      }
      try {
        console.log('[TerminalView] Calling wsSendInput for special key...')
        await wsSendInput(sessionId, '', key)
        console.log('[TerminalView] wsSendInput for special key completed')
      } catch (e) {
        console.error('[TerminalView] sendSpecialKey failed:', e)
        throw e
      }
    },
  }
})

// 会话名称 - 从全局会话列表中查找
const sessionName = computed(() => {
  const activeId = connection.activeSessionId.value
  if (activeId) {
    // 优先从全局会话列表查找
    const found = connection.activeSessions.value.find((s: any) => s.id === activeId)
    if (found?.name) return found.name
  }
  return connection.currentDevice.value?.name || '终端'
})

// 清空终端
function handleClear() {
  const sessionId = connection.activeSessionId.value
  if (sessionId) {
    globalClearTerminal(sessionId)
  }
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
  // 清空由全局管理器处理，这里只是响应事件
}

// KeepAlive 恢复时触发，确保终端实例正确显示
// 不清空缓冲区，保持现有数据继续接收新输出
function onTerminalActivated() {
  console.log('[TerminalView] onTerminalActivated, terminal instance preserved by global manager')
}

// ==================== TerminalInputBar Event Handlers ====================

async function onSubmit(text: string) {
  console.log('[TerminalView] onSubmit:', text)
  if (!terminalInstance.value) {
    console.warn('[TerminalView] No terminal instance for submit')
    return
  }
  try {
    await terminalInstance.value.sendInput(text)
  } catch (e) {
    console.error('[TerminalView] onSubmit failed:', e)
  }
}

async function onExecute(text: string) {
  console.log('[TerminalView] onExecute:', text)
  if (!terminalInstance.value) {
    console.warn('[TerminalView] No terminal instance for execute')
    return
  }
  try {
    await terminalInstance.value.sendInputWithEnter(text)
  } catch (e) {
    console.error('[TerminalView] onExecute failed:', e)
  }
}

async function onSpecialKey(key: string) {
  console.log('[TerminalView] onSpecialKey:', key)
  if (!terminalInstance.value) {
    console.warn('[TerminalView] No terminal instance for special key')
    return
  }
  try {
    await terminalInstance.value.sendSpecialKey(key)
  } catch (e) {
    console.error('[TerminalView] onSpecialKey failed:', e)
  }
}

// 监听连接状态变化
watch(() => connection.connectionStatus.value, (newStatus, oldStatus) => {
  if ((newStatus === 'connected' || newStatus === 'paired') &&
      (oldStatus === 'disconnected' || oldStatus === 'error' || oldStatus === undefined)) {
    console.log('[TerminalView] Reconnected')
    // 全局管理器会在 loadActiveSessions 时创建实例
  }
})

onMounted(async () => {
  // 路由参数是 :id，表示会话 ID
  const sessionId = route.params.id as string
  if (sessionId) {
    connection.activeSessionId.value = sessionId

    // 确保会话有离屏实例（可能已由 loadActiveSessions 创建）
    if (!hasTerminal(sessionId)) {
      createHiddenTerminal(sessionId)
    }

    // 如果已连接，订阅会话以开始接收输出
    if (isConnectedValue.value) {
      wsJoinSession(sessionId).catch(e => console.error('[TerminalView] wsJoinSession failed:', e))
    }
  }
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