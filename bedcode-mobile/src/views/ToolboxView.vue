<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- ==================== 插件视图二级页 ==================== -->
    <template v-if="activePluginView">
      <header class="flex-shrink-0 bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center gap-3">
        <button
          class="flex-shrink-0 p-1 -ml-1 text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-accent)] active:opacity-80 transition-colors"
          @click="activePluginView = null"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h1 class="flex-1 text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide truncate">{{ activePluginView.title }}</h1>
      </header>
      <div class="flex-1 overflow-hidden min-h-0">
        <PluginViewHost :plugin-id="activePluginView.pluginId" :component="activePluginView.component" />
      </div>
    </template>

    <!-- ==================== 入口列表 ==================== -->
    <template v-else>
      <!-- Header -->
      <header class="flex-shrink-0 bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center justify-between">
        <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ t('mobile.toolbox.title') }}</h1>
      </header>

      <div class="flex-1 overflow-y-auto p-4 space-y-3">
        <!-- 预设任务入口 -->
        <button
          class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 flex items-center gap-3 shadow-[var(--mobile-card-shadow)] hover:border-[var(--mobile-border-hover)] hover:shadow-[var(--mobile-card-shadow-hover)] active:scale-[0.99] transition-all duration-300 text-left group"
          @click="router.push({ name: 'mobile-preset-tasks' })"
        >
          <div class="w-12 h-12 rounded-xl flex items-center justify-center flex-shrink-0 bg-[var(--mobile-accent-muted)] border border-[var(--mobile-accent)]/20">
            <svg class="w-6 h-6 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <p class="font-medium text-[0.9375rem] text-[var(--mobile-text-primary)]">{{ t('mobile.toolbox.presetTasks') }}</p>
            <p class="text-[var(--mobile-text-muted)] text-sm truncate mt-0.5">{{ presetEntryDesc }}</p>
          </div>
          <!-- 任务数徽章 -->
          <span
            v-if="taskCount > 0"
            class="flex-shrink-0 inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full text-[11px] font-medium bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]"
          >
            {{ taskCount }}
          </span>
          <svg class="w-5 h-5 flex-shrink-0 text-[var(--mobile-text-disabled)] group-hover:text-[var(--mobile-accent)] transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <!-- 插件工具箱视图入口 -->
        <button
          v-for="view in pluginRegistry.toolboxViews.value"
          :key="view.viewId"
          class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 flex items-center gap-3 shadow-[var(--mobile-card-shadow)] hover:border-[var(--mobile-border-hover)] hover:shadow-[var(--mobile-card-shadow-hover)] active:scale-[0.99] transition-all duration-300 text-left group"
          @click="activePluginView = view"
        >
          <div class="w-12 h-12 rounded-xl flex items-center justify-center flex-shrink-0 bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] text-xl">
            🧩
          </div>
          <div class="flex-1 min-w-0">
            <p class="font-medium text-[0.9375rem] text-[var(--mobile-text-primary)] truncate">{{ view.title }}</p>
            <p class="text-[var(--mobile-text-muted)] text-sm truncate mt-0.5">{{ t('mobile.toolbox.pluginEntry') }}</p>
          </div>
          <svg class="w-5 h-5 flex-shrink-0 text-[var(--mobile-text-disabled)] group-hover:text-[var(--mobile-accent)] transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * ToolboxView - 工具箱入口页
 *
 * 一级入口列表：预设任务（跳转二级页面）+ 插件工具箱视图（页内二级展示）
 */

import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { usePresetTasks } from '@/composables/usePresetTasks'
import { getPluginRegistry } from '@/plugin/registry'
import PluginViewHost from '@/plugin/components/PluginViewHost.vue'

const router = useRouter()
const { t } = useI18n()
const { tasks, load } = usePresetTasks()
const pluginRegistry = getPluginRegistry()

/** 工具箱视图条目类型（从 registry 响应式数组推导） */
type ToolboxViewEntry = (typeof pluginRegistry.toolboxViews.value)[number]

/** 当前展开的插件工具箱视图（null = 入口列表） */
const activePluginView = ref<ToolboxViewEntry | null>(null)

const taskCount = computed(() => tasks.value.length)

/** 预设任务入口描述：有任务显示数量，无任务显示引导文案 */
const presetEntryDesc = computed(() =>
  taskCount.value > 0
    ? t('mobile.toolbox.presetEntryCount', { count: taskCount.value })
    : t('mobile.toolbox.presetEntryEmpty')
)

onMounted(async () => {
  await load()
})
</script>
