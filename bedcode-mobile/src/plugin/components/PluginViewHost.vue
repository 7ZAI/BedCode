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
import { provide } from 'vue'
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
</script>
