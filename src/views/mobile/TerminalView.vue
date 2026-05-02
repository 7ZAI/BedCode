<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3 flex items-center gap-3">
      <button @click="goBack" class="p-2 -ml-2">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="flex-1">
        <h1 class="font-semibold">{{ deviceName }}</h1>
        <div class="flex items-center gap-1.5 text-xs">
          <div
            :class="[
              'w-1.5 h-1.5 rounded-full',
              connection.isConnected.value ? 'bg-green-500' : 'bg-red-500'
            ]"
          ></div>
          <span class="text-dark-400">{{ connection.isConnected.value ? '已连接' : '未连接' }}</span>
        </div>
      </div>
      <button
        class="p-2 rounded-lg bg-dark-700 text-dark-300"
        @click="showSessionSelect = true"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h7" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output -->
    <div class="flex-1 overflow-hidden">
      <OutputRenderer
        :blocks="outputBlocks"
        :raw-output="rawOutput"
        :auto-scroll="autoScroll"
      />
    </div>

    <!-- Input Bar -->
    <InputBar
      ref="inputBarRef"
      :is-connected="connection.isConnected.value"
      :show-status="true"
      placeholder="输入消息..."
      @submit="handleSendInput"
      @special-key="handleSendSpecialKey"
    />

    <!-- Session Select Modal -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showSessionSelect" class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/60" @click="showSessionSelect = false"></div>
          <div class="relative w-full max-w-sm bg-dark-800 rounded-2xl p-4 max-h-[70vh] overflow-auto">
            <h3 class="font-semibold mb-3">选择会话</h3>

            <div v-if="terminal.sessions.value.length === 0" class="text-center py-8">
              <p class="text-dark-500 text-sm">暂无活跃会话</p>
            </div>

            <div v-else class="space-y-2">
              <button
                v-for="session in terminal.sessions.value"
                :key="session.id"
                :class="[
                  'w-full p-3 rounded-lg text-left',
                  terminal.currentSessionId.value === session.id ? 'bg-primary-900 text-primary-300' : 'bg-dark-700'
                ]"
                @click="handleSelectSession(session.id)"
              >
                <p class="font-medium">{{ session.name }}</p>
                <p class="text-dark-400 text-sm">{{ session.status }}</p>
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'
import { useOutputParser } from '@/composables/useOutputParser'
import OutputRenderer from '@/components/mobile/OutputRenderer.vue'
import InputBar from '@/components/mobile/InputBar.vue'

const router = useRouter()
const route = useRoute()

const connection = useRemoteConnection()
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
})

const {
  blocks: outputBlocks,
  rawOutput,
  parseOutput,
  clearOutput,
} = useOutputParser()

const inputBarRef = ref<InstanceType<typeof InputBar> | null>(null)
const autoScroll = ref(true)
const showSessionSelect = ref(false)

const deviceName = computed(() => connection.currentDevice.value?.name || 'Claude Code')

// 监听输出缓冲区，更新显示
watch(() => terminal.outputBuffer.value, (buffer) => {
  // 将缓冲区内容合并并解析
  const output = buffer.join('\n')
  clearOutput()
  parseOutput(output)
}, { deep: true })

// 监听等待输入状态
watch(() => terminal.isWaitingInput.value, (waiting) => {
  if (waiting) {
    inputBarRef.value?.focus()
  }
})

onMounted(async () => {
  const deviceId = route.params.deviceId as string

  // 启用自动重连恢复
  terminal.enableAutoReconnect()

  // 如果未连接，先连接
  if (connection.state.value.status !== 'connected' && connection.state.value.status !== 'paired') {
    const device = connection.pairedDevices.value.find(d => d.id === deviceId)
    if (device) {
      try {
        await connection.connect(device)
      } catch (error) {
        console.error('Failed to connect:', error)
        return
      }
    }
  }

  // 加载远程会话
  await terminal.loadSessions()

  // 如果有会话，自动选择第一个
  if (terminal.sessions.value.length > 0) {
    await terminal.joinSession(terminal.sessions.value[0].id)
  } else {
    showSessionSelect.value = true
  }
})

onUnmounted(async () => {
  // 禁用自动重连并离开会话
  terminal.disableAutoReconnect()
  await terminal.leaveSession()
})

function goBack() {
  router.push('/mobile/devices')
}

function handleSendInput(text: string) {
  terminal.sendInput(text)
  // 显示用户输入
  parseOutput(`\n> ${text}\n`)
}

function handleSendSpecialKey(key: string) {
  terminal.sendSpecialKey(key)
}

async function handleSelectSession(sessionId: string) {
  await terminal.joinSession(sessionId)
  showSessionSelect.value = false
  clearOutput()
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
