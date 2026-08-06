<template>
  <div v-if="items.length > 0" class="flex items-center gap-2 px-2" style="-webkit-app-region: no-drag;">
    <button
      v-for="item in items"
      :key="`${item.pluginId}:${item.id}`"
      class="flex items-center gap-1 px-1.5 py-0.5 text-xs text-slate-500 dark:text-dark-400 hover:text-slate-700 dark:hover:text-dark-200 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors"
      :title="item.label"
      @click="item.onClick?.()"
    >
      <span v-if="item.icon" class="plugin-icon">{{ item.icon }}</span>
      <span>{{ item.label }}</span>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginTitleBarItems — 渲染插件注册的标题栏项
 */
import { getPluginRegistry } from '../registry'

const registry = getPluginRegistry()
const items = registry.titleBarItems
</script>

<style scoped>
.plugin-icon {
  font-size: calc(14px * var(--ui-scale));
  line-height: 1;
}
</style>
