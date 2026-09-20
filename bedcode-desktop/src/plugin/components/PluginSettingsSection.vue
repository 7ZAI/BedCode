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
    <component :is="component" v-if="component" :shared="shared" />
  </section>
</template>

<script setup lang="ts">
/**
 * PluginSettingsSection — 宿主设置页里的插件贡献分组外壳
 *
 * 外层 `<section>` 与 `wb-section-title` 标题由宿主统一渲染，插件组件只渲染卡片
 * 正文 —— 保证贡献分组与内置分组像素一致，且标题一律走 i18n（插件命名空间）。
 * 与 PluginViewHost 同款：向子树 provide('pluginContext')，供插件组件取上下文。
 */
import { computed, provide } from 'vue'
import i18n from '@/locales'
import { getPluginRegistry } from '../registry'
import type { SettingsSharedState } from '@/composables/useSettingsSections'

const props = defineProps<{
  pluginId: string
  titleKey: string
  icon?: string
  component: any
  shared: SettingsSharedState
}>()

const registry = getPluginRegistry()

const context = registry.getContext(props.pluginId)
if (context) {
  provide('pluginContext', context)
}

// 标题按插件命名空间解析（与 context.i18n.t 同一前缀规则）；
// 插件未注册该 key 时 vue-i18n 回退显示 key 本身，不额外兜底文案
const title = computed(() => i18n.global.t(`${props.pluginId}.${props.titleKey}`))
</script>
