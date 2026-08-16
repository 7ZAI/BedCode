<template>
  <template v-if="items.length > 0">
    <div class="w-px h-4 bg-[var(--border)] mx-0.5"></div>
    <button
      v-for="item in items"
      :key="`${item.pluginId}:${item.id}`"
      class="wb-btn-ghost"
      :title="item.label"
      @click="item.onClick?.()"
    >
      <span v-if="item.icon" class="w-3.5 h-3.5 plugin-icon">{{ item.icon }}</span>
      <span v-else class="text-xs">{{ item.label }}</span>
    </button>
  </template>
</template>

<script setup lang="ts">
/**
 * PluginPageToolbar — 渲染插件注册的页面工具栏项
 * 接收目标页面标识（sessions/devices/...），只渲染注册到该页面的项
 */
import { computed } from 'vue'
import { getPluginRegistry } from '../registry'

const props = defineProps<{
  /** 目标页面标识，与 PageToolbarItemDescriptor.target 对应 */
  target: string
}>()

const registry = getPluginRegistry()

const items = computed(() => registry.pageToolbarItems.value.filter(item => item.target === props.target))
</script>

<style scoped>
.plugin-icon {
  font-size: calc(14px * var(--ui-scale));
  line-height: 1;
}
</style>
