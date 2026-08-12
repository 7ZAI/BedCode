<template>
  <aside
    class="bg-[var(--bg-sidebar)] flex flex-col border-r border-[var(--border)] flex-shrink-0 relative"
    :style="{ width: isResizing ? `${dragWidth}px` : (collapsed ? `${COLLAPSED_WIDTH}px` : `${EXPANDED_WIDTH}px`) }"
    :class="!isResizing && 'transition-[width] duration-200 ease'"
  >
    <nav class="flex-1 py-4 overflow-y-auto overflow-x-hidden px-3">
      <!-- ==================== SECTION: NAVIGATION（内置 + 插件统一排序） ==================== -->
      <h4 v-if="!collapsed" class="wb-sidebar-section px-2 mb-2">{{ $t('desktop.sidebar.navigation') }}</h4>
      <ul class="space-y-0.5" :class="collapsed && 'mt-1'">
        <li v-for="item in menuItems" :key="item.id">
          <router-link
            :to="item.path"
            class="flex items-center gap-2.5 h-9 rounded-md transition-colors duration-200"
            :class="[
              isActive(item)
                ? 'bg-[var(--bg-card)] font-medium text-[var(--text-primary)] shadow-sm'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]',
              collapsed ? 'justify-center px-0' : 'px-2.5'
            ]"
            :title="collapsed ? itemLabel(item) : undefined"
          >
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="item.icon" />
            </svg>
            <span v-if="!collapsed" class="text-[calc(13px*var(--ui-scale))] whitespace-nowrap">{{ itemLabel(item) }}</span>
          </router-link>
        </li>
      </ul>
    </nav>

    <!-- 底部状态条 + 折叠/展开按钮 -->
    <div class="p-2 border-t border-[var(--border)]">
      <!-- 展开状态：服务状态指示灯 + 状态文字 + 折叠按钮 -->
      <div
        v-if="!collapsed"
        class="flex items-center gap-2 px-1.5 h-8 rounded-md hover:bg-[var(--bg-hover)] transition-colors"
      >
        <span
          class="w-2 h-2 rounded-full flex-shrink-0 transition-colors duration-200"
          :class="statusDotClass"
        ></span>
        <span class="flex-1 min-w-0 text-xs font-medium text-[var(--text-secondary)] truncate">{{ statusText }}</span>
        <button
          class="w-7 h-7 flex items-center justify-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="$t('desktop.sidebar.collapse')"
          @click="toggleSidebar()"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
      </div>
      <!-- 折叠状态：仅展开按钮（服务状态指示灯隐藏） -->
      <div v-else class="flex justify-center">
        <button
          class="w-7 h-7 flex items-center justify-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
          :title="$t('desktop.sidebar.expand')"
          @click="toggleSidebar()"
        >
          <svg class="w-3.5 h-3.5 rotate-180" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 拖拽 resize handle（保留原有功能） -->
    <div
      class="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-[color:color-mix(in_srgb,var(--color-primary)_20%,transparent)] active:bg-[color:color-mix(in_srgb,var(--color-primary)_30%,transparent)] transition-colors duration-150"
      @mousedown="onResizeStart"
    ></div>
  </aside>
</template>

<script setup lang="ts">
/**
 * 桌面端侧边栏 — Warm Workbench 风格：内置菜单与插件面板统一排序，240px 可折叠
 * 折叠时隐藏服务状态指示灯；保留折叠/拖拽 resize/状态轮询/插件面板功能
 */
import { onMounted, onUnmounted, computed } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useSidebarMenu, type SidebarMenuItem } from '@/composables/useSidebarMenu'
import { collapsed, toggleSidebar, useSidebarResize, COLLAPSED_WIDTH, EXPANDED_WIDTH } from '@/composables/useSidebar'
import { useServer } from '@/composables/useServer'

const route = useRoute()
const { t } = useI18n()
const { menuItems } = useSidebarMenu()

const { isResizing, dragWidth, onResizeStart } = useSidebarResize()
const { status, loadStatus } = useServer()

/** 状态轮询定时器 — 轻量级 get_server_status，检测后台崩溃等外部状态变化 */
let statusTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await loadStatus()
  statusTimer = setInterval(loadStatus, 5000)
})

onUnmounted(() => {
  if (statusTimer) { clearInterval(statusTimer); statusTimer = null }
})

function isActive(item: SidebarMenuItem) {
  return item.prefix ? route.path.startsWith(item.path) : route.path === item.path
}

/** 菜单项显示文本：内置项为 i18n key，插件/自定义项为纯文本标题 */
function itemLabel(item: SidebarMenuItem) {
  return item.isI18nKey ? t(item.labelKey) : item.labelKey
}

/** 状态指示圆点颜色 */
const statusDotClass = computed(() => {
  switch (status.value) {
    case 'running': return 'bg-green-500'
    case 'starting': return 'bg-yellow-500'
    default: return 'bg-gray-400'
  }
})

/** 状态文本（i18n） */
const statusText = computed(() => {
  switch (status.value) {
    case 'running': return t('desktop.sidebar.serviceRunning')
    case 'starting': return t('desktop.sidebar.serviceStarting')
    default: return t('desktop.sidebar.serviceStopped')
  }
})
</script>
