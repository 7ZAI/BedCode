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
import { computed, provide, watch } from 'vue'
import { getPluginRegistry } from '../registry'

const props = defineProps<{
  viewId: string
  pluginId: string
}>()

const registry = getPluginRegistry()

// 同步 provide：确保在 setup 阶段完成，子组件 inject 可用
// context 在插件激活时已设置（路由守卫确保激活完成才导航）
const context = registry.getContext(props.pluginId)
if (context) {
  provide('pluginContext', context)
}

// 双保险：vue-router 切换同一路由记录（/plugin/sidebar/:pluginId/:viewId）时会复用
// 组件实例且 setup 不重跑，此处 watch 参数变化重新 provide，避免子组件拿到旧插件 context
watch(
  () => [props.pluginId, props.viewId],
  () => {
    const ctx = registry.getContext(props.pluginId)
    if (ctx) {
      provide('pluginContext', ctx)
    }
  }
)

const resolvedComponent = computed(() =>
  registry.getViewComponent(props.pluginId, props.viewId)
)
</script>
