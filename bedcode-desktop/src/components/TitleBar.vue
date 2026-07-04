<template>
  <div
    class="titlebar h-9 bg-white dark:bg-dark-800 flex items-center justify-between select-none border-b border-slate-200 dark:border-dark-700 shadow-sm dark:shadow-none"
    data-tauri-drag-region
  >
    <!-- Left: drag region spacer -->
    <div class="flex items-center px-4" data-tauri-drag-region>
    </div>

    <!-- Plugin Title Bar Extension -->
    <PluginTitleBarItems />

    <!-- Right: Window Controls -->
    <div class="flex items-center titlebar-buttons">
      <button
        @click="minimize"
        class="w-12 h-9 flex items-center justify-center hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
      >
        <svg class="w-4 h-4 text-slate-600 dark:text-dark-300 hover:text-slate-900 dark:hover:text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
        </svg>
      </button>
      <button
        @click="toggleMaximize"
        class="w-12 h-9 flex items-center justify-center hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
      >
        <svg v-if="!isMaximized" class="w-3.5 h-3.5 text-slate-600 dark:text-dark-300 hover:text-slate-900 dark:hover:text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <rect x="4" y="4" width="16" height="16" rx="1" stroke-width="2" />
        </svg>
        <svg v-else class="w-3.5 h-3.5 text-slate-600 dark:text-dark-300 hover:text-slate-900 dark:hover:text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <rect x="2" y="6" width="14" height="14" rx="1" stroke-width="2" />
          <path stroke-width="2" d="M6 6V4a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1h-2" />
        </svg>
      </button>
      <button
        @click="close"
        class="w-12 h-9 flex items-center justify-center hover:bg-red-600 hover:text-white transition-colors"
      >
        <svg class="w-4 h-4 text-slate-600 dark:text-dark-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import PluginTitleBarItems from '@/plugin/components/PluginTitleBarItems.vue'

const appWindow = getCurrentWindow()
const isMaximized = ref(false)

async function checkMaximized() {
  try {
    isMaximized.value = await appWindow.isMaximized()
  } catch (e) {
    console.error('Failed to check maximized state:', e)
  }
}

onMounted(() => {
  checkMaximized()
  // Listen for window resize to update maximized state
  window.addEventListener('resize', checkMaximized)
})

onUnmounted(() => {
  window.removeEventListener('resize', checkMaximized)
})

async function minimize() {
  try {
    await appWindow.minimize()
  } catch (e) {
    console.error('Failed to minimize:', e)
  }
}

async function toggleMaximize() {
  try {
    await appWindow.toggleMaximize()
    // Wait a bit for the window state to update
    setTimeout(checkMaximized, 100)
  } catch (e) {
    console.error('Failed to toggle maximize:', e)
  }
}

async function close() {
  try {
    await appWindow.close()
  } catch (e) {
    console.error('Failed to close:', e)
  }
}
</script>

<style scoped>
.titlebar {
  -webkit-app-region: drag;
}

.titlebar-buttons {
  -webkit-app-region: no-drag;
}

.titlebar-buttons button {
  -webkit-app-region: no-drag;
}
</style>
