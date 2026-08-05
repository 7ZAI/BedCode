<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- ==================== 插件视图二级页 ==================== -->
    <template v-if="activePluginView">
      <div class="page-header flex-shrink-0">
        <div class="flex items-center gap-3">
          <button
            class="flex-shrink-0 p-1 -ml-1 transition-colors active:opacity-80"
            style="color: var(--mobile-text-secondary)"
            @click="activePluginView = null"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
            </svg>
          </button>
          <h1 class="flex-1 page-title truncate">{{ activePluginView.title }}</h1>
        </div>
      </div>
      <div class="flex-1 overflow-hidden min-h-0">
        <PluginViewHost :plugin-id="activePluginView.pluginId" :component="activePluginView.component" />
      </div>
    </template>

    <!-- ==================== 入口列表 ==================== -->
    <template v-else>
      <div class="page-header flex-shrink-0">
        <h1 class="page-title">{{ t('mobile.toolbox.title') }}</h1>
        <p class="page-subtitle">{{ t('mobile.toolbox.subtitle') || '任务与插件工具入口' }}</p>
      </div>

      <div class="flex-1 overflow-y-auto px-4 pb-8 space-y-6">
        <!-- 预设任务入口 -->
        <section>
          <h2 class="group-section-title">{{ t('mobile.toolbox.tools') || '工具' }}</h2>
          <div class="group-card">
            <button class="group-row group-row-btn" @click="router.push({ name: 'mobile-preset-tasks' })">
              <span class="icon-chip chip-cyan">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
                </svg>
              </span>
              <div class="flex-1 min-w-0">
                <div class="group-row-title">{{ t('mobile.toolbox.presetTasks') }}</div>
                <div class="group-row-sub mt-0.5">{{ presetEntryDesc }}</div>
              </div>
              <span v-if="taskCount > 0" class="status-badge badge-cyan">{{ taskCount }}</span>
              <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </button>
          </div>
        </section>

        <!-- 插件工具箱视图入口 -->
        <section v-if="pluginRegistry.toolboxViews.value.length > 0">
          <h2 class="group-section-title">{{ t('mobile.toolbox.pluginViews') || '插件视图' }}</h2>
          <div class="group-card">
            <button
              v-for="view in pluginRegistry.toolboxViews.value"
              :key="view.viewId"
              class="group-row group-row-btn"
              @click="activePluginView = view"
            >
              <PluginViewHost
                v-if="view.entry"
                :plugin-id="view.pluginId"
                :component="view.entry"
                class="flex-1 min-w-0"
              />
              <template v-else>
                <span class="icon-chip chip-violet">
                  <svg v-if="isSvgIcon(view.icon)" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="view.icon" />
                  </svg>
                  <span v-else class="text-base">{{ view.icon ?? '🧩' }}</span>
                </span>
                <div class="flex-1 min-w-0">
                  <div class="group-row-title truncate">{{ view.title }}</div>
                  <div class="group-row-sub mt-0.5">{{ t('mobile.toolbox.pluginEntry') }}</div>
                </div>
              </template>
              <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </button>
          </div>
        </section>
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

/** 判断 icon 是否为 SVG path d（以 M 开头视为路径数据） */
function isSvgIcon(icon?: string): boolean {
  return typeof icon === 'string' && icon.startsWith('M')
}

onMounted(async () => {
  await load()
})
</script>
