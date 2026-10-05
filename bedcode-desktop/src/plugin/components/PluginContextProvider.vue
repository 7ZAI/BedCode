<!--
  PluginContextProvider — 插件上下文注入壳（keyed Provider）

  必须有真实元素根，禁止退化成裸 `<slot />`：宿主 DesktopLayout 的路由出口是
  `<Transition name="page">`，Transition 要求子组件渲染**元素根**。裸 slot 透传会让
  Vue 报 `Component inside <Transition> renders non-element root node that cannot be
  animated`；历史上配合 `mode="out-in"` 时更严重——leave 钩子挂在不会被 unmount 处理的
  Fragment vnode 上 → afterLeave 永不触发 → 切走插件视图时主区域**永久白屏**
  （2026-09-25 实测）。同理，本文件顶部的说明必须是模板**外**注释——模板根级注释
  在 dev 编译下会被保留，与 div 一起构成多根（Fragment），把根重新变成非元素节点。

  rootClass 默认 `contents`（display: contents，不生成布局盒子）：设置分组等纯内容
  场景零布局影响；需要参与页面过渡动画与高度链路的宿主（PluginViewHost）传 `h-full`。
-->
<template>
  <div :class="props.rootClass">
    <slot />
  </div>
</template>

<script setup lang="ts">
/**
 * 用法：`<PluginContextProvider :key="registry.contextIdentity(pluginId)" :plugin-id="pluginId" root-class="h-full">`
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

const props = withDefaults(
  defineProps<{
    pluginId: string
    /**
     * 根元素 class —— 决定本组件是「零布局影响的内容透传层」（`contents`，默认）
     * 还是「参与过渡动画/高度链路的包装层」（`h-full`，PluginViewHost 用）。
     */
    rootClass?: string
  }>(),
  { rootClass: 'contents' },
)

const context = getPluginRegistry().getContext(props.pluginId)
if (context) {
  provide('pluginContext', context)
}
</script>
