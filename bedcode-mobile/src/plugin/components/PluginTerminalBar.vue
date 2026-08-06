<template>
  <template v-for="item in toolbarItems" :key="`${item.pluginId}:${item.id}`">
    <button
      class="tool-btn"
      @click="handleClick(item)"
      :title="item.label"
    >
      <span v-if="item.icon" class="text-sm">{{ item.icon }}</span>
      <span v-else class="text-xs">{{ item.label }}</span>
    </button>
  </template>

  <!-- Auto Task Panel -->
  <AutoTaskPanelHost
    :visible="showAutoTaskPanel"
    :active-session-id="activeSessionId ?? ''"
    @close="showAutoTaskPanel = false"
  />
</template>

<script setup lang="ts">
/**
 * PluginTerminalBar — 终端工具栏插件项渲染 + 面板管理
 */
import { ref } from 'vue'
import { getPluginRegistry } from '@/plugin/registry'
import { useMobileConnection } from '@/composables/useMobileConnection'
import AutoTaskPanelHost from '@/plugin/auto-task/AutoTaskPanelHost.vue'

const toolbarItems = getPluginRegistry().terminalToolbarItems
const { activeSessionId } = useMobileConnection()
const showAutoTaskPanel = ref(false)

function handleClick(item: any) {
  if (item.id === 'auto-task-toolbar') {
    showAutoTaskPanel.value = !showAutoTaskPanel.value
  } else {
    item.onClick?.()
  }
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
