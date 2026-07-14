<template>
  <template v-for="item in toolbarItems" :key="item.id">
    <button
      class="tool-btn"
      @click="item.onClick?.()"
      :title="item.label"
    >
      <span class="text-xs">{{ item.label }}</span>
    </button>
  </template>
</template>

<script setup lang="ts">
/**
 * PluginTerminalBar — 终端工具栏插件项渲染
 */
import { getPluginRegistry } from '@/plugin/registry'

const toolbarItems = getPluginRegistry().terminalToolbarItems
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
