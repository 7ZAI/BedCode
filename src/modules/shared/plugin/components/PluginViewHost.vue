<template>
  <component
    :is="resolvedComponent"
    v-if="resolvedComponent"
  />
  <div v-else class="p-4 text-sm text-slate-500">
    {{ $t('desktop.plugin.viewNotFound') }}
  </div>
</template>

<script setup lang="ts">
/**
 * PluginViewHost — 动态渲染插件 Vue 组件
 */
import { computed } from 'vue'
import { getPluginRegistry } from '../registry'

const props = defineProps<{
  viewId: string
  pluginId: string
}>()

const registry = getPluginRegistry()
const resolvedComponent = computed(() =>
  registry.getViewComponent(props.pluginId, props.viewId)
)
</script>
