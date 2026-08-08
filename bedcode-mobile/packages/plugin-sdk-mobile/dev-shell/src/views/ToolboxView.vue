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
  })
}
</script>

<template>
  <div class="p-4">
    <h2 class="text-base font-semibold mb-3 text-[var(--mobile-text-primary)]">
      {{ t('devshell.toolbox.title') }}
    </h2>

    <div v-if="toolboxPages.length === 0" class="py-12 flex flex-col items-center gap-2 text-center">
      <span class="text-3xl">🧰</span>
      <p class="text-sm text-[var(--mobile-text-secondary)]">{{ t('devshell.toolbox.empty') }}</p>
      <p class="text-xs text-[var(--mobile-text-muted)] px-8">{{ t('devshell.toolbox.emptyHint') }}</p>
    </div>

    <div class="grid gap-3" style="grid-template-columns: repeat(2, minmax(0, 1fr))">
      <template v-for="entry in toolboxPages" :key="entry.pluginId + entry.page.id">
        <!-- 插件自定义入口卡片：宿主内联渲染，自带交互 -->
        <div
          v-if="entry.page.entry"
          class="rounded-xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] overflow-hidden"
          @click="openPage(entry.pluginId, entry.page.id)"
        >
          <PluginComponent :plugin-id="entry.pluginId" :component="entry.page.entry" />
        </div>
        <!-- 默认统一卡片 -->
        <button
          v-else
          class="rounded-xl bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] hover:border-[var(--mobile-border-hover)] active:scale-[0.98] p-4 text-left transition-all duration-200 flex flex-col items-center gap-2 min-h-[104px]"
          @click="openPage(entry.pluginId, entry.page.id)"
        >
          <span v-if="isSvgIcon(entry.page.icon)" class="w-7 h-7 flex items-center justify-center text-[var(--mobile-accent)]">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-7 h-7">
              <path :d="entry.page.icon" />
            </svg>
          </span>
          <span v-else class="text-2xl leading-none">{{ entry.page.icon || '🧩' }}</span>
          <span class="text-sm font-medium text-[var(--mobile-text-primary)] truncate max-w-full">
            {{ entry.page.title }}
          </span>
          <span class="text-[11px] text-[var(--mobile-text-muted)] truncate max-w-full">
            {{ entry.pluginId }}
          </span>
        </button>
      </template>
    </div>
  </div>
</template>
