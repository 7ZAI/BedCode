<script setup lang="ts">
/**
 * PluginView — 插件视图渲染容器（工具箱页 / 路由 / 设置区 / navTab）
 *
 * 由 activeView 驱动：页头（back + title）+ 插件组件（provide pluginContext）。
 */
import { useI18n } from 'vue-i18n'
import { activeView, goBackView } from '../registry'
import PluginComponent from '../components/PluginComponent.vue'

const { t } = useI18n()
</script>

<template>
  <div class="h-full flex flex-col min-h-0">
    <div
      v-if="activeView && activeView.header !== false"
      class="flex items-center gap-2 px-4 h-11 flex-shrink-0 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/80"
    >
      <button
        class="w-8 h-8 flex-shrink-0 flex items-center justify-center rounded-lg text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-text-primary)] transition-colors duration-200"
        aria-label="back"
        @click="goBackView()"
      >
        ←
      </button>
      <span class="text-sm font-semibold truncate min-w-0">{{ activeView.title || t('devshell.back') }}</span>
    </div>
    <div class="flex-1 min-h-0 overflow-y-auto">
      <PluginComponent
        v-if="activeView"
        :plugin-id="activeView.pluginId"
        :component="activeView.component"
      />
    </div>
  </div>
</template>
