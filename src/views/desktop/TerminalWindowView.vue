<template>
  <div class="h-screen flex flex-col bg-dark-900">
    <!-- Header with title and window controls -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-2 flex items-center justify-between h-10 shrink-0" data-tauri-drag-region>
      <div class="flex items-center gap-2 text-sm text-dark-300">
        <span class="font-medium">{{ sessionName }}</span>
      </div>
      <div class="flex items-center gap-1">
        <button @click="minimizeWindow" class="p-1.5 hover:bg-dark-700 rounded transition-colors" title="最小化">
          <svg class="w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
          </svg>
        </button>
        <button @click="toggleMaximize" class="p-1.5 hover:bg-dark-700 rounded transition-colors" title="最大���">
          <svg class="w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path v-if="!isMaximized" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h4" />
            <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5 5m5-5l5-5m-5 5v-4.5m0 4.5h4.5" />
          </svg>
        </button>
        <button @click="closeWindow" class="p-1.5 hover:bg-red-600 rounded transition-colors" title="关闭">
          <svg class="w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Terminal Preview Component -->
    <TerminalPreview :session="session" :show-input="true" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import TerminalPreview from '@/components/desktop/TerminalPreview.vue'
import type { SessionInfo } from '@/composables/useTauri'

const route = useRoute()
const sessionId = ref(route.params.id as string)
const sessionName = ref('')
const session = ref<SessionInfo | null>(null)
const isMaximized = ref(false)

async function loadSessionInfo() {
  try {
    const result = await invoke<SessionInfo>('get_session', { sessionId: sessionId.value })
    session.value = result
    sessionName.value = result.name
  } catch (e) {
    console.error('[TerminalWindow] Failed to load session info:', e)
    sessionName.value = '终端'
  }
}

async function minimizeWindow() {
  const win = getCurrentWindow()
  await win.minimize()
}

async function toggleMaximize() {
  const win = getCurrentWindow()
  const maximized = await win.isMaximized()
  if (maximized) {
    await win.unmaximize()
    isMaximized.value = false
  } else {
    await win.maximize()
    isMaximized.value = true
  }
}

async function closeWindow() {
  const win = getCurrentWindow()
  await win.close()
}

onMounted(() => {
  loadSessionInfo()
})
</script>

<style scoped>
:deep(.xterm) {
  height: 100%;
  padding: 8px;
}
:deep(.xterm-viewport) {
  border-radius: 0;
}
</style>