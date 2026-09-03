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
      </div>

      <div class="flex-1 overflow-y-auto px-4 pb-8">
        <div class="pt-2 space-y-3">
          <!-- 预设任务入口 -->
          <button
            class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
            @click="router.push({ name: 'mobile-preset-tasks' })"
          >
            <div class="flex items-start gap-3">
              <span class="toolbox-icon chip-cyan">
                <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
                </svg>
              </span>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate">{{ t('mobile.toolbox.presetTasks') }}</span>
                  <span v-if="taskCount > 0" class="status-badge badge-cyan">{{ taskCount }}</span>
                </div>
                <p class="text-xs mt-1 leading-relaxed text-[var(--mobile-text-secondary)] line-clamp-2">{{ presetEntryDesc }}</p>
              </div>
              <svg class="w-4 h-4 flex-shrink-0 mt-1" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </div>
          </button>

          <!-- 插件工具箱视图入口 -->
          <button
            v-for="view in toolboxEntries"
            :key="`${view.pluginId}:${view.viewId}`"
            class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
            @click="activePluginView = view"
          >
            <PluginViewHost
              v-if="view.entry"
              :plugin-id="view.pluginId"
              :component="view.entry"
              class="flex-1 min-w-0"
            />
            <template v-else>
              <div class="flex items-start gap-3">
                <span class="toolbox-icon chip-violet">
                  <svg v-if="isSvgIcon(view.icon)" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="view.icon" />
                  </svg>
                  <span v-else class="text-2xl">{{ view.icon ?? '🧩' }}</span>
                </span>
                <div class="flex-1 min-w-0">
                  <div class="text-base font-medium text-[var(--mobile-text-primary)] truncate">{{ view.title }}</div>
                  <p class="text-xs mt-1 leading-relaxed text-[var(--mobile-text-secondary)] line-clamp-2">{{ t('mobile.toolbox.pluginEntry') }}</p>
                </div>
                <svg class="w-4 h-4 flex-shrink-0 mt-1" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                </svg>
              </div>
            </template>
          </button>

          <!-- 插件区空态：无插件工具箱视图时的次级入口占位，避免孤岛空白 -->
          <div
            v-if="toolboxEntries.length === 0"
            class="flex items-center gap-3 w-full border border-dashed border-[var(--mobile-border-hover)] rounded-xl px-4 py-5"
          >
            <span class="toolbox-icon chip-zinc">
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z" />
              </svg>
            </span>
            <div class="flex-1 min-w-0">
              <div class="text-base font-medium text-[var(--mobile-text-primary)]">{{ t('mobile.toolbox.pluginViews') }}</div>
              <p class="text-xs mt-1 leading-relaxed text-[var(--mobile-text-secondary)]">{{ t('mobile.toolbox.pluginEmptyHint') }}</p>
            </div>
            <button
              class="flex-shrink-0 h-11 px-4 rounded-lg text-xs font-medium transition-colors active:opacity-80"
              style="background: var(--mobile-accent-muted); color: var(--mobile-accent)"
              @click="router.push({ name: 'mobile-plugins' })"
            >
              {{ t('mobile.toolbox.pluginManage') }}
            </button>
          </div>
        </div>
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

import { ref, computed, onMounted, onActivated } from 'vue'
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

/**
 * KeepAlive reactivate 强制刷新：bump refreshTick → toolboxEntries 重算返回
 * 新数组 → v-for 重渲染，确保插件停用/重启用期间 registry 变更在返回工具箱后
 * 必反映到 DOM（防御缓存组件 reactivate 不重渲染的边界，对齐用户「返回时刷新
 * 工具箱」预期）。即便 Vue 正常 reactivate 会重渲染，此机制为边界兼底保障。
 */
const refreshTick = ref(0)
onActivated(() => {
  refreshTick.value++
})

/** 入口列表源：依赖 refreshTick + registry，reactivate 时返回新数组触发重渲染 */
const toolboxEntries = computed<ToolboxViewEntry[]>(() => {
  void refreshTick.value
  return [...pluginRegistry.toolboxViews.value]
})

/**
 * 当前展开的插件工具箱视图（null = 入口列表）。
 *
 * writable computed：getter 按引用相等复核 activeViewRef 暂存对象是否仍在
 * toolboxEntries（registry 现存入口）。插件停用→重启用时 loader 重新
 * registerToolboxPage 用【新对象】替换入口（同 pluginId 不同引用），旧对象
 * 不在 → getter 返回 null → 二级页隐藏、入口列表呈现。
 */
const activeViewRef = ref<ToolboxViewEntry | null>(null)
const activePluginView = computed<ToolboxViewEntry | null>({
  get: () => {
    void refreshTick.value
    const v = activeViewRef.value
    return v && toolboxEntries.value.includes(v) ? v : null
  },
  set: (val) => {
    activeViewRef.value = val
  },
})

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

<style scoped>
/* 工具箱图标容器：与 PluginIcon md 尺寸一致（48px, rounded-xl） */
.toolbox-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 3rem;
  height: 3rem;
  border-radius: 0.75rem;
  flex-shrink: 0;
}
</style>
