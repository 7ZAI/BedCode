<template>
  <slot />
</template>

<script setup lang="ts">
/**
 * PluginContextProvider — 插件上下文注入壳（keyed Provider）
 *
 * 用法：`<PluginContextProvider :key="registry.contextIdentity(pluginId)" :plugin-id="pluginId">`
 * 由宿主视图外壳（PluginViewHost / PluginSettingsSection）在「插件上下文身份变化」时
 * 重挂载本组件，使 `provide('pluginContext')` 在 **setup 同步执行** 里重新注入最新 context。
 *
 * 为什么必须重挂载而不是 watch：Vue 的 `provide()` **只能在 setup 同步调用**，
 * watch 回调里调用实测不生效（Vue 3.5，见 PluginViewHost 旧实现的历史债）——旧代码
 * 在 props 变化 watch 里 re-provide 是死代码，插件二次激活（context 对象被替换）后
 * 子树持续注入停用前（通道令牌已回收）的旧 context，插件面命令全员被拒
 * （`session.config.list` / `session.list` → 「会话数据加载失败」toast）。
 */
import { provide } from 'vue'
import { getPluginRegistry } from '../registry'

const props = defineProps<{ pluginId: string }>()

const context = getPluginRegistry().getContext(props.pluginId)
if (context) {
  provide('pluginContext', context)
}
</script>