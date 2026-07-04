<template>
  <template v-if="items.length > 0">
    <div class="w-px h-4 bg-slate-200 dark:bg-dark-600 mx-1"></div>
    <Button
      v-for="item in items"
      :key="item.id"
      variant="ghost"
      size="sm"
      :title="item.label"
      @click="item.onClick?.()"
    >
      <span v-if="item.icon" v-html="item.icon" class="w-4 h-4" />
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
