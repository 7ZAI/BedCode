<template>
  <header
    class="h-12 bg-[var(--bg-card)] border-b border-[var(--border)] flex items-center justify-between select-none flex-shrink-0"
    data-tauri-drag-region
  >
    <!-- 左：logo + 名称 -->
    <div class="flex items-center gap-3 px-4" data-tauri-drag-region>
      <div class="w-6 h-6 rounded-md bg-[#1C1917] dark:bg-[#FAF9F7] flex items-center justify-center">
        <span class="text-xs font-bold leading-none text-[#FAF9F7] dark:text-[#1C1917]">B</span>
      </div>
      <span class="text-[calc(13px*var(--ui-scale))] font-semibold tracking-tight text-[var(--text-primary)]">BedCode</span>
    </div>

    <!-- 插件标题栏扩展点 -->
    <div class="titlebar-buttons">
      <PluginTitleBarItems />
    </div>

    <!-- 右：窗口控制 -->
    <div class="flex items-center pr-1">
      <div class="flex items-center titlebar-buttons">
        <button
          @click="minimize"
          class="w-9 h-8 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="t('desktop.terminal.minimize')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path stroke-linecap="round" d="M20 12H4" /></svg>
        </button>
        <button
          @click="toggleMaximize"
          class="w-9 h-8 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="t('desktop.terminal.maximize')"
        >
          <svg v-if="!isMaximized" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><rect x="4" y="4" width="16" height="16" rx="1" /></svg>
          <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><rect x="2" y="6" width="14" height="14" rx="1" /><path d="M6 6V4a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1h-2" /></svg>
        </button>
        <button
          @click="close"
          class="w-9 h-8 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[#B42318] hover:text-white transition-colors"
          :title="t('desktop.terminal.close')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path stroke-linecap="round" d="M6 18L18 6M6 6l12 12" /></svg>
        </button>
      </div>
    </div>
  </header>
</template>

<script setup lang="ts">
/**
 * 标题栏 — Warm Workbench 风格：48px 工具栏式，左品牌，右窗口控制
 * 保留插件标题栏扩展点
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import PluginTitleBarItems from '@/plugin/components/PluginTitleBarItems.vue'

const { t } = useI18n()
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
header {
  -webkit-app-region: drag;
}

.titlebar-buttons,
.titlebar-buttons button {
  -webkit-app-region: no-drag;
}
</style>
