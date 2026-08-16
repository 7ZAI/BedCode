<template>
  <div class="h-full overflow-y-auto overflow-x-hidden">
    <PluginViewHost :plugin-id="pluginId" :component="component" />
  </div>
</template>

<script setup lang="ts">
/**
 * PluginNavTabHost — 导航 Tab 插件视图容器
 */
import PluginViewHost from './PluginViewHost.vue'

defineProps<{
  pluginId: string
  component: any
}>()
</script>
