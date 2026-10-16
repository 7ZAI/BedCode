<template>
  <section>
    <!-- 图标可选：无图标时与内置分组标题行形态完全一致（不引入 flex 布局差异） -->
    <h3 class="wb-section-title" :class="icon ? 'flex items-center gap-2' : undefined">
      <svg
        v-if="icon"
        class="w-4 h-4 flex-shrink-0"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="icon" />
      </svg>
      <span>{{ title }}</span>
    </h3>
    <!-- 插件卡片正文经 keyed PluginContextProvider 注入 context：插件二次激活换
         新 context 对象时 Provider 重挂载并在 setup 重新 provide（provide 只能在
         setup 同步执行，watch 里不生效），卡片组件不再拿到停用前（令牌已回收）的旧 context -->
    <PluginContextProvider
      v-if="component && contextKey"
      :key="contextKey"
      :plugin-id="pluginId"
    >
      <component :is="component" :shared="shared" />
    </PluginContextProvider>
  </section>
</template>

<script setup lang="ts">
/**
 * PluginSettingsSection — 宿主设置页里的插件贡献分组外壳
 *
 * 外层 `<section>` 与 `wb-section-title` 标题由宿主统一渲染，插件组件只渲染卡片
 * 正文 —— 保证贡献分组与内置分组像素一致，且标题一律走 i18n（插件命名空间）。
 * 与 PluginViewHost 同款：向子树 provide('pluginContext')，供插件组件取上下文。
 * context 注入走 keyed PluginContextProvider，同因：插件二次激活后旧 provide
 * 不会自动刷新，卡片必须随新 context 重挂载（历史债细节见 PluginViewHost 文件头）。
 */
import { computed } from 'vue'
import i18n from '@/locales'
import { getPluginRegistry } from '../registry'
import PluginContextProvider from './PluginContextProvider.vue'
import type { SettingsSharedState } from '@/composables/useSettingsSections'

const props = defineProps<{
  pluginId: string
  titleKey: string
  icon?: string
  component: any
  shared: SettingsSharedState
}>()

const registry = getPluginRegistry()

/** 当前插件上下文身份（响应式；keyed Provider 的重挂载依据，见 contextIdentity 说明） */
const contextKey = computed(() => registry.contextIdentity(props.pluginId))

// 标题按插件命名空间解析（与 context.i18n.t 同一前缀规则）；
// 插件未注册该 key 时 vue-i18n 回退显示 key 本身，不额外兜底文案
const title = computed(() => i18n.global.t(`${props.pluginId}.${props.titleKey}`))
</script>
