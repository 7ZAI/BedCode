<template>
  <header
    class="h-10 bg-[var(--bg-card)] border-b border-[var(--border)] flex items-center justify-between select-none flex-shrink-0"
    data-tauri-drag-region
  >
    <!-- 左：logo + 名称 -->
    <div class="flex items-center gap-3 px-4" data-tauri-drag-region>
      <!-- 品牌图标：内联 src-tauri/icons/icon.svg，填充色随 light/dark 主题切换（浅色=深底浅纹，夜间=浅底深纹） -->
      <svg
        class="w-5 h-5 flex-shrink-0 [--logo-bg-start:#2E2A22] [--logo-bg-end:#0A0907] [--logo-fg:#FFFFFF] dark:[--logo-bg-start:#FAF9F7] dark:[--logo-bg-end:#E7E4DC] dark:[--logo-fg:#1C1917]"
        viewBox="0 0 100 100"
        aria-hidden="true"
      >
        <defs>
          <linearGradient id="titlebar-logo-bg" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="var(--logo-bg-start)" />
            <stop offset="100%" stop-color="var(--logo-bg-end)" />
          </linearGradient>
        </defs>
        <rect width="100" height="100" rx="18" fill="url(#titlebar-logo-bg)" />
        <path d="M 24 18 L 59 50 L 24 82 L 32 74 L 51 50 L 32 26 Z" fill="var(--logo-fg)" />
        <path d="M 51 60 L 84 62 L 53 65 Z" fill="var(--logo-fg)" />
      </svg>
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
          class="w-8 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="t('desktop.terminal.minimize')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path stroke-linecap="round" d="M20 12H4" /></svg>
        </button>
        <button
          @click="toggleMaximize"
          class="w-8 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="t('desktop.terminal.maximize')"
        >
          <svg v-if="!isMaximized" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><rect x="4" y="4" width="16" height="16" rx="1" /></svg>
          <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><rect x="2" y="6" width="14" height="14" rx="1" /><path d="M6 6V4a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1h-2" /></svg>
        </button>
        <button
          @click="close"
          class="w-8 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[#B42318] hover:text-white transition-colors"
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
 * 标题栏 — Warm Workbench 风格：40px 工具栏式，左品牌，右窗口控制
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
