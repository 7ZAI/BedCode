<script setup lang="ts">
/**
 * PanelView — 插件面板/页面渲染容器（侧边栏面板、工具箱页共用）
 */
import { useI18n } from 'vue-i18n'
import { activeView } from '../registry'
import PluginComponent from '../components/PluginComponent.vue'

defineEmits<{ back: [] }>()
const { t } = useI18n()
</script>

<template>
  <div class="h-full flex flex-col min-h-0">
    <div class="flex items-center gap-2 px-4 h-11 flex-shrink-0 border-b border-[var(--border)]">
      <button
        class="px-2 py-1 rounded-btn text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200"
        @click="$emit('back')"
      >
        ← {{ t('devshell.back') }}
      </button>
      <span class="text-sm font-semibold truncate min-w-0">{{ activeView?.title || '' }}</span>
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
