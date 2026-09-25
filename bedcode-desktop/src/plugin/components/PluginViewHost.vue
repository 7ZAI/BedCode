<template>
  <PluginContextProvider
    v-if="resolvedComponent && contextKey"
    :key="contextKey"
    :plugin-id="pluginId"
  >
    <component :is="resolvedComponent" />
  </PluginContextProvider>
  <div v-else class="p-4 text-sm text-slate-500">
    {{ $t('desktop.plugin.viewNotFound') }}
  </div>
</template>

<script setup lang="ts">
/**
 * PluginViewHost — 动态渲染插件 Vue 组件
 *
 * 解析插件注册的视图组件并提供 PluginContext 给子组件树。
 *
 * context 注入走 keyed PluginContextProvider（`contextIdentity` 作 key）：
 * - 插件**二次激活**换新 context 对象时 → key 变化 → Provider 重挂载 →
 *   `provide('pluginContext')` 在 setup 里重新注入最新 context，子树（重新注入后）
 *   不再持有停用前（通道令牌已回收）的旧 context，插件面命令不再全员被拒。
 * - 插件无 context（停用清理后 / 未激活）时 Provider 不渲染，视图保持「未找到」。
 *
 * 旧实现（历史债）：曾在 props 变化 watch 里 re-provide —— Vue 的 `provide()`
 * 只能在 setup 同步调用，watch 回调里调用实测（Vue 3.5）不生效，属死代码；
 * 插件二次启用后子树持续注入旧 context 是本文件的故障根因。
 */
import { computed } from 'vue'
import { getPluginRegistry } from '../registry'
import PluginContextProvider from './PluginContextProvider.vue'

const props = defineProps<{
  viewId: string
  pluginId: string
}>()

const registry = getPluginRegistry()

/** 当前插件上下文身份（响应式；走 contextsIndex 投影而非裸 Map，见 contextIdentity 说明） */
const contextKey = computed(() => registry.contextIdentity(props.pluginId))

const resolvedComponent = computed(() => registry.getViewComponent(props.pluginId, props.viewId))
</script>