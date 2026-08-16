<template>
  <template v-for="item in toolbarItems" :key="`${item.pluginId}:${item.id}`">
    <button
      class="tool-btn"
      @click="item.onClick?.()"
      :title="item.label"
    >
      <!-- SVG path：Heroicons outline 风格，viewBox=0 0 24 24 -->
      <svg v-if="item.icon && isSvgPath(item.icon)" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="item.icon" />
      </svg>
      <!-- Emoji fallback -->
      <span v-else-if="item.icon" class="text-sm">{{ item.icon }}</span>
      <span v-else class="text-xs">{{ item.label }}</span>
    </button>
  </template>
</template>

<script setup lang="ts">
/**
 * PluginTerminalBar — 终端工具栏插件项渲染
 * 点击直接回调插件注册的 onClick（面板等 UI 由插件自身挂载管理）
 */
import { getPluginRegistry } from '@/plugin/registry'

const toolbarItems = getPluginRegistry().terminalToolbarItems

/** 判断 icon 字符串是否为 SVG path data（以 M/m 开头，非 emoji） */
function isSvgPath(icon: string): boolean {
  return /^[Mm]\s*[\d.]/.test(icon.trim())
}
</script>

<style scoped>
.tool-btn {
  padding: 0.35rem 0.5rem;
  border-radius: 0.375rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  white-space: nowrap;
}

.tool-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}
</style>
