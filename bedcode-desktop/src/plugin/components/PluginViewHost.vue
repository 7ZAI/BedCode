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
import { computed, provide } from 'vue'
import { getPluginRegistry } from '../registry'

const props = defineProps<{
  viewId: string
  pluginId: string
}>()

const registry = getPluginRegistry()

// 从 registry 获取插件上下文，provide 给组件树
// 插件组件通过 inject('pluginContext') 获取
const context = registry.getContext(props.pluginId)
if (context) {
  provide('pluginContext', context)
}

const resolvedComponent = computed(() =>
  registry.getViewComponent(props.pluginId, props.viewId)
)
</script>
