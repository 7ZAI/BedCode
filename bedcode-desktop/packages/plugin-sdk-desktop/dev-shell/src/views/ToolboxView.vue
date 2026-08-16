<script setup lang="ts">
/**
 * ToolboxView — 插件工具箱入口网格（桌面端卡片）
 */
import { useI18n } from 'vue-i18n'
import { openActiveView, toolboxPages } from '../registry'
import { isSvgIcon } from '../utils/icon'

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
  <div class="p-6">
    <h2 class="text-lg font-semibold mb-4">{{ t('devshell.nav.toolbox') }}</h2>

    <div v-if="toolboxPages.length === 0" class="py-16 flex flex-col items-center gap-2 text-center">
      <span class="text-3xl">🧰</span>
      <p class="text-sm text-[var(--text-secondary)]">{{ t('devshell.toolbox.empty') }}</p>
      <p class="text-xs text-[var(--text-tertiary)] max-w-md">{{ t('devshell.toolbox.emptyHint') }}</p>
    </div>

    <div class="grid gap-4" style="grid-template-columns: repeat(auto-fill, minmax(180px, 1fr))">
      <button
        v-for="entry in toolboxPages"
        :key="entry.pluginId + entry.page.id"
        class="bg-card border border-[var(--border)] rounded-card p-4 text-left shadow-card hover:shadow-card-hover hover:border-[var(--border-strong)] transition-all duration-200 flex flex-col items-center gap-2"
        @click="openPage(entry.pluginId, entry.page.id)"
      >
        <span v-if="isSvgIcon(entry.page.icon)" class="w-8 h-8 flex items-center justify-center text-[var(--color-primary)]">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-8 h-8">
            <path :d="entry.page.icon" />
          </svg>
        </span>
        <span v-else class="text-2xl leading-none">{{ entry.page.icon || '🧩' }}</span>
        <span class="text-sm font-medium truncate max-w-full">{{ entry.page.title }}</span>
        <span class="text-[11px] text-[var(--text-tertiary)] truncate max-w-full">{{ entry.pluginId }}</span>
      </button>
    </div>
  </div>
</template>
