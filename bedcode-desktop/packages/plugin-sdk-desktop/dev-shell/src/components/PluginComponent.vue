<script setup lang="ts">
/**
 * PluginComponent — 插件视图容器（provide pluginContext）
 */
import { provide, watch } from 'vue'
import { getPluginRecord } from '../registry'

const props = defineProps<{
  pluginId: string
  component: any
}>()

function syncContext() {
  const record = getPluginRecord(props.pluginId)
  if (record?.context) provide('pluginContext', record.context)
}

syncContext()
watch(() => props.pluginId, syncContext)
</script>

<template>
  <component :is="component" />
</template>
