<template>
  <div v-if="items.length > 0" class="flex items-center gap-2">
    <button
      v-for="item in items"
      :key="item.id"
      class="flex items-center gap-1 px-2 py-0.5 text-xs text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors"
      @click="item.onClick?.()"
    >
      <span v-if="item.icon" v-html="item.icon" />
      <span>{{ item.label }}</span>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginStatusBar — 渲染插件注册的状态栏项
 */
import { getPluginRegistry } from '../registry'

const registry = getPluginRegistry()
const items = registry.statusbarItems
</script>
