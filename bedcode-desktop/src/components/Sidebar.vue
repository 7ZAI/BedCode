<template>
  <aside
    class="bg-[var(--bg-sidebar)] flex flex-col border-r border-[var(--border)] flex-shrink-0 relative"
    :style="{ width: isResizing ? `${dragWidth}px` : (collapsed ? `${COLLAPSED_WIDTH}px` : `${EXPANDED_WIDTH}px`) }"
    :class="!isResizing && 'transition-[width] duration-200 ease'"
  >
    <nav class="flex-1 py-4 overflow-y-auto overflow-x-hidden px-3">
      <!-- ==================== SECTION: NAVIGATION ==================== -->
      <h4 v-if="!collapsed" class="wb-sidebar-section px-2 mb-2">{{ $t('desktop.sidebar.navigation') }}</h4>
      <ul class="space-y-0.5" :class="collapsed && 'mt-1'">
        <li v-for="item in navItems" :key="item.path">
          <router-link
            :to="item.path"
            class="flex items-center gap-2.5 h-9 rounded-md transition-colors duration-150"
            :class="[
              isActive(item)
                ? 'bg-[var(--bg-hover)] font-medium text-[var(--text-primary)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]',
              collapsed ? 'justify-center px-0' : 'px-2.5'
            ]"
            :title="collapsed ? $t(item.labelKey) : undefined"
          >
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="item.icon" />
            </svg>
            <span v-if="!collapsed" class="text-[13px] whitespace-nowrap">{{ $t(item.labelKey) }}</span>
          </router-link>
        </li>
      </ul>

      <!-- ==================== SECTION: PLUGINS ==================== -->
      <template v-if="sidebarPlugins.length + toolboxPlugins.length > 0">
        <h4 v-if="!collapsed" class="wb-sidebar-section px-2 mt-6 mb-2">{{ $t('desktop.sidebar.plugins') }}</h4>
        <ul class="space-y-0.5" :class="collapsed && 'mt-6'">
          <li v-for="view in sidebarPlugins" :key="`plugin-${view.pluginId}-${view.viewId}`">
            <router-link
              :to="`/plugin/sidebar/${view.pluginId}/${view.viewId}`"
              class="flex items-center gap-2.5 h-9 rounded-md transition-colors duration-150"
              :class="[
                $route.path === `/plugin/sidebar/${view.pluginId}/${view.viewId}`
                  ? 'bg-[var(--bg-hover)] font-medium text-[var(--text-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]',
                collapsed ? 'justify-center px-0' : 'px-2.5'
              ]"
              :title="collapsed ? view.title : undefined"
            >
              <svg v-if="view.icon" class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="view.icon" />
              </svg>
              <svg v-else class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 6h16M4 12h16M4 18h7" />
              </svg>
              <span v-if="!collapsed" class="text-[13px] whitespace-nowrap">{{ view.title }}</span>
            </router-link>
          </li>
          <li v-for="view in toolboxPlugins" :key="`toolbox-${view.pluginId}-${view.viewId}`">
            <router-link
              :to="`/plugin/toolbox/${view.pluginId}/${view.viewId}`"
              class="flex items-center gap-2.5 h-9 rounded-md transition-colors duration-150"
              :class="[
                $route.path === `/plugin/toolbox/${view.pluginId}/${view.viewId}`
                  ? 'bg-[var(--bg-hover)] font-medium text-[var(--text-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]',
                collapsed ? 'justify-center px-0' : 'px-2.5'
              ]"
              :title="collapsed ? view.title : undefined"
            >
              <svg v-if="view.icon" class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="view.icon" />
              </svg>
              <svg v-else class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
              <span v-if="!collapsed" class="text-[13px] whitespace-nowrap">{{ view.title }}</span>
            </router-link>
          </li>
        </ul>
      </template>

      <!-- ==================== SECTION: STATUS ==================== -->
      <template v-if="!collapsed">
        <h4 class="wb-sidebar-section px-2 mt-6 mb-2">{{ $t('desktop.sidebar.status') }}</h4>
        <div class="px-2.5 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)]">
          <div class="flex items-center gap-2">
            <span class="w-2 h-2 rounded-full flex-shrink-0 transition-colors duration-200" :class="statusDotClass"></span>
            <span class="text-xs font-medium text-[var(--text-primary)]">{{ statusText }}</span>
          </div>
          <div class="wb-mono text-[11px] text-[var(--text-tertiary)] mt-2 space-y-1">
            <div class="flex justify-between"><span>port</span><span class="text-[var(--text-secondary)]">{{ port }}</span></div>
            <div class="flex justify-between"><span>ws</span><span class="text-[var(--text-secondary)]">{{ wsShort }}</span></div>
          </div>
        </div>
      </template>
    </nav>

    <!-- 底部折叠按钮 -->
    <div class="p-2 border-t border-[var(--border)]" :class="collapsed ? 'flex justify-center' : ''">
      <button
        class="w-full h-7 flex items-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
        :class="collapsed ? 'justify-center' : 'justify-end px-2.5'"
        :title="collapsed ? $t('desktop.sidebar.expand') : $t('desktop.sidebar.collapse')"
        @click="toggleSidebar()"
      >
        <svg class="w-3.5 h-3.5 transition-transform duration-200" :class="collapsed && 'rotate-180'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
    </div>

    <!-- 拖拽 resize handle（保留原有功能） -->
    <div
      class="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-[var(--color-primary)]/20 active:bg-[var(--color-primary)]/30 transition-colors duration-150"
      @mousedown="onResizeStart"
    ></div>
  </aside>
</template>

<script setup lang="ts">
/**
 * 桌面端侧边栏 — Warm Workbench 风格：Navigation/Plugins/Status 分组，240px 可折叠
 * 保留折叠/拖拽 resize/状态轮询/插件面板功能
 */
import { onMounted, onUnmounted, computed } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { getPluginRegistry } from '@/plugin/registry'
import { collapsed, toggleSidebar, useSidebarResize, COLLAPSED_WIDTH, EXPANDED_WIDTH } from '@/composables/useSidebar'
import { useServer } from '@/composables/useServer'

const route = useRoute()
const { t } = useI18n()
const pluginRegistry = getPluginRegistry()
const sidebarPlugins = pluginRegistry.sidebarViews
const toolboxPlugins = pluginRegistry.toolboxViews

const { isResizing, dragWidth, onResizeStart } = useSidebarResize()
const { status, port, loadStatus } = useServer()

/** 状态轮询定时器 — 轻量级 get_server_status，检测后台崩溃等外部状态变化 */
let statusTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await loadStatus()
  statusTimer = setInterval(loadStatus, 5000)
})

onUnmounted(() => {
  if (statusTimer) { clearInterval(statusTimer); statusTimer = null }
})

interface NavItem {
  path: string
  labelKey: string
  icon: string
  /** true 时用 startsWith 匹配（插件页等多级路由） */
  prefix?: boolean
}

/** 导航项：顺序与原型 Warm Workbench 一致 */
const navItems: NavItem[] = [
  {
    path: '/sessions',
    labelKey: 'desktop.sidebar.session',
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01',
  },
  {
    path: '/server',
    labelKey: 'desktop.sidebar.server',
    icon: 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01',
  },
  {
    path: '/devices',
    labelKey: 'desktop.sidebar.devicePairing',
    icon: 'M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z',
  },
  {
    path: '/plugins',
    labelKey: 'desktop.plugin.title',
    prefix: true,
    icon: 'M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z',
  },
  {
    path: '/settings',
    labelKey: 'desktop.sidebar.settings',
    icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z',
  },
]

function isActive(item: NavItem) {
  return item.prefix ? route.path.startsWith(item.path) : route.path === item.path
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

/** WebSocket 短标签（技术术语，保留英文） */
const wsShort = computed(() => {
  switch (status.value) {
    case 'running': return 'active'
    case 'starting': return 'starting'
    default: return 'inactive'
  }
})
</script>
