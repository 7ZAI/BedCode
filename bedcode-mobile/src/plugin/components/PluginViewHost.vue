<template>
  <div class="plugin-view-host h-full">
    <component :is="component" v-if="component" />
    <div v-else class="flex items-center justify-center h-full text-[var(--mobile-text-disabled)] text-sm">
      {{ t('mobile.plugin.loadFailed') }}
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginViewHost — 插件视图容器
 *
 * provide PluginContext 给插件组件树，渲染插件注册的 Vue 组件
 */
import { provide, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { getPluginRegistry } from '@/plugin/registry'

const props = defineProps<{
  pluginId: string
  component: any
}>()

const { t } = useI18n()

// provide PluginContext 给插件组件
const context = getPluginRegistry().getContext(props.pluginId)
if (context) {
  provide('pluginContext', context)
}

// 双保险：宿主在切换插件视图时若复用本组件实例（pluginId 变化但 setup 不重跑），
// 此处 watch 重新 provide，避免子组件 inject 到旧插件的 PluginContext
watch(
  () => props.pluginId,
  () => {
    const ctx = getPluginRegistry().getContext(props.pluginId)
    if (ctx) {
      provide('pluginContext', ctx)
    }
  }
)
</script>
