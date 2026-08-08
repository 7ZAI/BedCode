<script setup lang="ts">
/**
 * ToolboxView — 插件工具箱入口网格（与宿主同款卡片：图标 + 标题 + 插件名）
 *
 * 支持插件自定义入口卡片（ToolboxPageDescriptor.entry，宿主内联渲染）；
 * 点击进入插件视图（activeView → PluginView 渲染）。
 */
import { useI18n } from 'vue-i18n'
import { openActiveView, toolboxPages } from '../registry'
import { isSvgIcon } from '../utils/icon'
import PluginComponent from '../components/PluginComponent.vue'

const { t } = useI18n()

function openPage(pluginId: string, pageId: string) {
  const entry = toolboxPages.value.find((p) => p.pluginId === pluginId && p.page.id === pageId)
  if (!entry) return
  openActiveView({
    kind: 'toolbox',
    pluginId,
    title: entry.page.title,
    component: entry.page.component,
    // header: false — 与 navTab 约定一致：由 AppShell 全局页头统一提供
    // back + 标题，避免 PluginView 再渲染一个页头造成重复
    header: false,
  })
}
</script>

<template>
  <!-- 与宿主 ToolboxView 同款：全宽入口卡片列表（无重复页面标题，标题由 AppShell 页头提供） -->
  <div class="px-4 py-3">
    <!-- 空态：无任何插件时展示 -->
    <div v-if="toolboxPages.length === 0" class="py-16 flex flex-col items-center gap-2 text-center">
      <span class="text-3xl">🧰</span>
      <p class="text-sm text-[var(--mobile-text-secondary)]">{{ t('devshell.toolbox.empty') }}</p>
      <p class="text-xs text-[var(--mobile-text-muted)] px-8">{{ t('devshell.toolbox.emptyHint') }}</p>
    </div>

    <div v-else class="space-y-3">
      <template v-for="entry in toolboxPages" :key="entry.pluginId + entry.page.id">
        <!-- 插件自定义入口卡片：宿主内联渲染，自带交互；按压反馈与宿主一致 -->
        <div
          v-if="entry.page.entry"
          class="rounded-xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] overflow-hidden cursor-pointer transition-[border-color,opacity] duration-300 hover:border-[var(--mobile-border-hover)] active:opacity-90"
          @click="openPage(entry.pluginId, entry.page.id)"
        >
          <PluginComponent :plugin-id="entry.pluginId" :component="entry.page.entry" />
        </div>
        <!-- 默认统一卡片：与宿主同款横向行（图标 chip + 标题 + 插件名 + chevron） -->
        <button
          v-else
          class="w-full flex items-center gap-3 p-4 text-left rounded-xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] cursor-pointer transition-[border-color,opacity] duration-300 hover:border-[var(--mobile-border-hover)] active:opacity-90"
          @click="openPage(entry.pluginId, entry.page.id)"
        >
          <span class="icon-chip chip-cyan flex-shrink-0">
            <svg v-if="isSvgIcon(entry.page.icon)" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="entry.page.icon" />
            </svg>
            <span v-else class="text-xl">{{ entry.page.icon || '🧩' }}</span>
          </span>
          <span class="flex-1 min-w-0">
            <span class="block text-sm font-medium text-[var(--mobile-text-primary)] truncate">
              {{ entry.page.title }}
            </span>
            <span class="block mt-0.5 text-xs text-[var(--mobile-text-muted)] truncate">
              {{ entry.pluginId }}
            </span>
          </span>
          <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </template>
    </div>
  </div>
</template>
