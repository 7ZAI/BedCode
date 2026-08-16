<script setup lang="ts">
/**
 * SettingsPage — 设置二级页 (Mobile)
 *
 * 自行渲染页头（返回 + 标题），不使用宿主 SettingsSubPage 包裹，
 * 避免与 ToolboxView 已有的 < 文件传输 页头叠加产生双重 header。
 *
 * 宿主 PluginRouteView 以无 props 方式渲染组件（header: false 裸渲染模式），
 * 本组件负责实例化 useSettings 并把 settingsApi + t 注入 SettingsSection。
 * 入口：FileTransferView 齿轮经 context.ui.openPage('settings') 整体路由跳转，
 * 返回按钮经 context.ui.goBack() 回到浏览页。
 */
import { inject, onMounted } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import { useSettings } from '../composables/useSettings'
import SettingsSection from './SettingsSection.vue'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)
const settingsApi = useSettings(context)

onMounted(() => {
  void settingsApi.load()
})
</script>

<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- 页头：自行渲染，避免与 ToolboxView 的页头叠加 -->
    <header class="flex-shrink-0 flex items-center gap-3 px-4 pt-3 pb-2 bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)]">
      <button
        class="flex-shrink-0 p-1 -ml-1 text-[var(--mobile-text-secondary)] active:opacity-80 transition-colors"
        @click="context.ui.goBack()"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h1 class="flex-1 min-w-0 page-title truncate">{{ t('transfer.settings.title') }}</h1>
    </header>

    <!-- 设置内容 -->
    <div class="flex-1 overflow-y-auto overflow-x-hidden">
      <SettingsSection :settings-api="settingsApi" :t="t" />
    </div>
  </div>
</template>
