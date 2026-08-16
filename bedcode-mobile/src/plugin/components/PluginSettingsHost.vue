<template>
  <div class="px-4 py-3">
    <PluginViewHost :plugin-id="pluginId" :component="component" />
  </div>
</template>

<script setup lang="ts">
/**
 * PluginSettingsHost — 设置页插件视图容器
 */
import PluginViewHost from './PluginViewHost.vue'

defineProps<{
  pluginId: string
  component: any
}>()
</script>
