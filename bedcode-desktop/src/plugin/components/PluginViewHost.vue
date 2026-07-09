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
 *
 * 解析插件注册的视图组件并提供 PluginContext 给子组件树
 */
import { computed, watchEffect, provide } from 'vue'
import { getPluginRegistry } from '../registry'

const props = defineProps<{
  viewId: string
  pluginId: string
}>()

const registry = getPluginRegistry()

// 响应式 provide：插件 context 可能在组件挂载后才设置（懒激活场景）
watchEffect(() => {
  const context = registry.getContext(props.pluginId)
  if (context) {
    provide('pluginContext', context)
  }
})

const resolvedComponent = computed(() =>
  registry.getViewComponent(props.pluginId, props.viewId)
)
</script>
