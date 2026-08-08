<script setup lang="ts">
/**
 * PluginComponent — 插件视图容器
 *
 * provide('pluginContext') 给插件组件树（与宿主 PluginViewHost 行为一致），
 * 插件组件经 inject('pluginContext') 取上下文；切换插件时 watch 重新 provide。
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
