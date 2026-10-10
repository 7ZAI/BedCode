<template>
  <div class="plugin-context-host h-full min-h-0">
    <component :is="component" v-if="component" />
    <div v-else class="flex items-center justify-center h-full text-[var(--mobile-text-disabled)] text-sm">
      {{ t('devshell.plugin.loadFailed') }}
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginContextHost — 插件运行面容器（宿主 PluginViewHost 的 dev-shell 同形）
 * -----------------------------------------------------------------------------
 * provide('pluginContext') 给插件组件树（与宿主 `src/plugin/components/PluginViewHost.vue`
 * 行为一致），插件组件经 inject('pluginContext') 取上下文。
 *
 * 存在的理由与宿主相同：壳直渲染应用运行面，不认识插件，因此「上下文注入」这件事
 * 必须由插件侧自己包一层——壳里不能出现任何插件形态的细节。
 */
import { provide, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { getPluginRecord } from '../../registry'

const props = defineProps<{
  pluginId: string
  component: any
}>()

const { t } = useI18n()

function syncContext(): void {
  const record = getPluginRecord(props.pluginId)
  if (record?.context) provide('pluginContext', record.context)
}

// 双保险：壳复用本组件实例（pluginId 变化但 setup 不重跑）时重新 provide，
// 否则子组件会 inject 到上一个插件的 context
syncContext()
watch(() => props.pluginId, syncContext)
</script>