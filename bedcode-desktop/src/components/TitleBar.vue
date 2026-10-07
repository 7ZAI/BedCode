<template>
  <header
    class="h-10 bg-[var(--bg-card)] border-b border-[var(--border)] flex items-center justify-between select-none flex-shrink-0"
    data-tauri-drag-region
  >
    <!-- 左：logo + 名称 -->
    <div class="flex items-center gap-3 px-4" data-tauri-drag-region>
      <!-- 品牌图标：内联 src-tauri/icons/icon.svg，填充色随 light/dark 主题切换（浅色=深底浅纹，夜间=浅底深纹） -->
      <svg
        class="w-5 h-5 flex-shrink-0 [--logo-bg-start:#16181C] [--logo-bg-end:#08090B] [--logo-fg:#F5F7F9] [--logo-accent:#FF9E2C] dark:[--logo-bg-start:#FAF9F7] dark:[--logo-bg-end:#E7E4DC] dark:[--logo-fg:#16181C] dark:[--logo-accent:#C2701A]"
        viewBox="0 0 100 100"
        aria-hidden="true"
      >
        <defs>
          <linearGradient id="titlebar-logo-bg" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="var(--logo-bg-start)" />
            <stop offset="100%" stop-color="var(--logo-bg-end)" />
          </linearGradient>
        </defs>
        <rect width="100" height="100" rx="22" fill="url(#titlebar-logo-bg)" />
        <!-- WasmApp 标识：W 由四段彼此分离的笔画构成（多个隔离应用），中峰嵌琥珀核心（活跃实例） -->
        <g
          stroke="var(--logo-fg)"
          stroke-width="13"
          stroke-linecap="butt"
          fill="none"
        >
          <line x1="23.2" y1="33" x2="36.8" y2="67" />
          <line x1="39.3" y1="67.1" x2="48.7" y2="46.9" />
          <line x1="51.3" y1="46.9" x2="60.7" y2="67.1" />
          <line x1="63.2" y1="67" x2="76.8" y2="33" />
        </g>
        <path
          d="M50 36.5 L56.5 43 L50 49.5 L43.5 43 Z"
          fill="var(--logo-accent)"
        />
      </svg>
      <span
        class="text-[calc(13px*var(--ui-scale))] font-semibold tracking-tight text-[var(--text-primary)]"
        >WasmApp</span
      >
    </div>

    <!-- 插件标题栏扩展点 -->
    <div class="titlebar-buttons">
      <PluginTitleBarItems />
    </div>

    <!-- 右：窗口控制 -->
    <div class="flex items-center pr-1">
      <div class="flex items-center titlebar-buttons">
        <button
          class="w-8 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="t('desktop.terminal.minimize')"
          @click="minimize"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
          >
            <path stroke-linecap="round" d="M20 12H4" />
          </svg>
        </button>
        <button
          class="w-8 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="t('desktop.terminal.maximize')"
          @click="toggleMaximize"
        >
          <svg
            v-if="!isMaximized"
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
          >
            <rect x="4" y="4" width="16" height="16" rx="1" />
          </svg>
          <svg
            v-else
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
          >
            <rect x="2" y="6" width="14" height="14" rx="1" />
            <path d="M6 6V4a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1h-2" />
          </svg>
        </button>
        <button
          class="w-8 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[#B42318] hover:text-white transition-colors"
          :title="t('desktop.terminal.close')"
          @click="close"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
          >
            <path stroke-linecap="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
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
import { logger } from '@/utils/frontendLogger'
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
    logger.error('Failed to check maximized state:', e)
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
    logger.error('Failed to minimize:', e)
  }
}

async function toggleMaximize() {
  try {
    await appWindow.toggleMaximize()
    // Wait a bit for the window state to update
    setTimeout(checkMaximized, 100)
  } catch (e) {
    logger.error('Failed to toggle maximize:', e)
  }
}

async function close() {
  try {
    await appWindow.close()
  } catch (e) {
    logger.error('Failed to close:', e)
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
