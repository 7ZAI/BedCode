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
      />
    </div>

    <!-- Input Assistant 悬浮球 -->
    <InputAssistant
      :terminal-ref="terminalRef"
      :terminal-instance="null"
      :is-connected="isConnectedValue"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, inject, type Ref } from 'vue'

// 定义组件名称，用于 KeepAlive 缓存
defineOptions({
  name: 'MobileTerminal',
})

import { useRouter, useRoute } from 'vue-router'
import { useMobileConnection } from '@/modules/shared/composables/useMobileConnection'
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

const terminalRef = ref<any>(null)

// 会话名称
const sessionName = computed(() => {
  return connection.currentDevice.value?.name || 'Claude Code'
})

// 清空终端
function handleClear() {
  outputBuffer.value = ''
  terminalRef.value?.clear()
}

// Terminal ready handler
function onTerminalReady() {
  console.log('Mobile terminal ready')
}

// Terminal resize handler
function handleTerminalResize(cols: number, rows: number) {
  // TODO: 实现调整终端大小
  console.log('Terminal resize:', cols, rows)
}

// Terminal clear handler
function onTerminalClear() {
  outputBuffer.value = ''
}

onMounted(async () => {
  // TODO: 实现完整的 terminal 功能
})

onUnmounted(async () => {
  // 清理
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