<template>
  <!-- shrink-0 + whitespace-nowrap：标题栏按钮不收缩不换行（旧实现允许收缩，
       窗口偏窄时多字标签会被挤成两行、溢出 40px 工具条）；字号随 --ui-scale -->
  <div
    v-if="items.length > 0"
    class="flex items-center gap-1 px-2 shrink-0"
    style="-webkit-app-region: no-drag"
  >
    <button
      v-for="item in items"
      :key="`${item.pluginId}:${item.id}`"
      class="flex items-center gap-1 h-6 px-2 shrink-0 whitespace-nowrap rounded-[6px] text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
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
