<template>
  <template v-if="items.length > 0">
    <div class="w-px h-4 bg-slate-200 dark:bg-dark-600 mx-1"></div>
    <Button
      v-for="item in items"
      :key="`${item.pluginId}:${item.id}`"
      variant="ghost"
      size="sm"
      :title="item.label"
      @click="item.onClick?.()"
    >
      <span v-if="item.icon" class="w-4 h-4 plugin-icon">{{ item.icon }}</span>
      <span v-else class="text-xs">{{ item.label }}</span>
    </Button>
  </template>
</template>

<script setup lang="ts">
/**
 * PluginTerminalToolbar — 渲染插件注册的终端工具栏项
 */
import { getPluginRegistry } from '../registry'
import Button from '@/components/Button.vue'

const registry = getPluginRegistry()
const items = registry.terminalToolbarItems
</script>

<style scoped>
.plugin-icon {
  font-size: calc(14px * var(--ui-scale));
  line-height: 1;
}
</style>
