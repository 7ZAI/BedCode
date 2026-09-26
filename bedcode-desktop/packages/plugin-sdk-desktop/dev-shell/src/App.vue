<script setup lang="ts">
/**
 * AppShell（桌面端）— 桌面页面骨架（与宿主 DesktopLayout / TitleBar / Sidebar 同构）
 *
 * 标题栏（40px + BedCode logo + 插件 titleBar 项）→ 侧边栏（内置导航分组 + 插件
 * sidebar 面板，按 order 排序；可折叠）→ 主内容区（activeView 或当前 Tab）→
 * 状态栏（连接状态 + 插件 statusBar 项）。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { toast, Toaster, type ToasterProps } from 'vue-sonner'
import {
  activeView,
  openActiveView,
  plugins,
  sidebarPanels,
  statusBarItems,
  titleBarItems,
} from './registry'
import { deactivateAll } from './loader'
import { connected } from './mock/session'
import { isSvgIcon } from './utils/icon'
import { saveLocale, type DevLocale } from './locale'
import { useHostUi } from './theme'
import PanelView from './views/PanelView.vue'
import ToolboxView from './views/ToolboxView.vue'
import PluginsView from './views/PluginsView.vue'
import TerminalView from './views/TerminalView.vue'
// dev-shell 调试视图：TerminalInputRail 组件可视化测试（引用宿主源码）
import TerminalInputRailDemo from './views/TerminalInputRailDemo.vue'
import SettingsView from './views/SettingsView.vue'
import LogPanel from './components/LogPanel.vue'
// SDK 共享全局弹窗宿主（与宿主 DesktopLayout 同组件）
import PluginGlobalDialog from '../../src/ui/PluginGlobalDialog.vue'

type BaseTab = 'terminal' | 'toolbox' | 'plugins' | 'settings' | 'rail'
const activeTab = ref<BaseTab>('toolbox')
const logOpen = ref(false)

// 侧边栏折叠状态（与宿主 useSidebar 同宽口径：展开 240px / 折叠 56px）
const collapsed = ref(false)
const COLLAPSED_WIDTH = 56

function toggleCollapsed() {
  collapsed.value = !collapsed.value
}

const { t, locale } = useI18n()
const { theme } = useHostUi()

// Toaster 配置与宿主 App.vue 保持一致（expand 防重叠、visible-toasts 放宽批量通知）；
// 主题跟随 dev-shell 的宿主界面设置（浅色/深色/跟随系统，与宿主 useSettings 同语义）
const toasterTheme = computed(() => theme.value as ToasterProps['theme'])
const toastOptions: ToasterProps['toastOptions'] = {
  classes: {
    toast: '!rounded-[10px] !shadow-lg',
    title: '!text-[calc(13px*var(--ui-scale))] !font-medium',
    description: '!text-[var(--text-secondary)]',
    closeButton:
      '!bg-transparent !border-transparent !text-[var(--text-secondary)] hover:!text-[var(--text-primary)]',
  },
}

/** 顶栏演示按钮：连发 3 条不同级别 toast，用于验证进出场动画与多 toast 堆叠 */
function fireDemoToasts() {
  toast.success('成功：传输队列已入队', { duration: 5000 })
  toast.error('错误：对端拒绝同名文件', { duration: 5000 })
  toast.info('信息：文件传输完成', { duration: 5000 })
}

// 语言切换选项（语言名用自身文字展示，无需翻译）
const localeOptions: { value: DevLocale; label: string }[] = [
  { value: 'zh-CN', label: '中' },
  { value: 'en', label: 'EN' },
]

function setLocale(next: DevLocale) {
  locale.value = next
  saveLocale(next)
}

// 插件名列表：分隔符跟随当前语言
const pluginSummary = computed(() =>
  plugins.value.map((p) => p.name).join(locale.value === 'en' ? ', ' : '，'),
)

// 内置导航图标：Heroicons outline（viewBox 0 0 24 24，与宿主 Sidebar 菜单同一图标体系）
const TERMINAL_ICON =
  'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z'
const TOOLBOX_ICON =
  'M3.75 6A2.25 2.25 0 016 3.75h2.25A2.25 2.25 0 0110.5 6v2.25a2.25 2.25 0 01-2.25 2.25H6a2.25 2.25 0 01-2.25-2.25V6zM3.75 15.75A2.25 2.25 0 016 13.5h2.25a2.25 2.25 0 012.25 2.25V18a2.25 2.25 0 01-2.25 2.25H6A2.25 2.25 0 013.75 18v-2.25zM13.5 6a2.25 2.25 0 012.25-2.25H18A2.25 2.25 0 0120.25 6v2.25A2.25 2.25 0 0118 10.5h-2.25a2.25 2.25 0 01-2.25-2.25V6zM13.5 15.75a2.25 2.25 0 012.25-2.25H18a2.25 2.25 0 012.25 2.25V18A2.25 2.25 0 0118 20.25h-2.25A2.25 2.25 0 0113.5 18v-2.25z'
// 与宿主 useSidebarMenu 内置项同源的图标（plugins / settings）
const PLUGINS_ICON =
  'M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z'
const SETTINGS_ICON =
  'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z'
const RAIL_ICON = 'M4 6h16M4 12h16M4 18h16'

const baseTabs = computed<{ key: BaseTab; label: string; icon: string; order: number }[]>(() => [
  { key: 'terminal' as const, label: t('devshell.nav.terminal'), icon: TERMINAL_ICON, order: 100 },
  { key: 'toolbox' as const, label: t('devshell.nav.toolbox'), icon: TOOLBOX_ICON, order: 200 },
  { key: 'plugins' as const, label: t('devshell.nav.plugins'), icon: PLUGINS_ICON, order: 300 },
  { key: 'settings' as const, label: t('devshell.nav.settings'), icon: SETTINGS_ICON, order: 400 },
  // 调试专用：TerminalInputRail 组件测试页
  { key: 'rail' as const, label: t('devshell.nav.rail'), icon: RAIL_ICON, order: 500 },
])

const sidebarItems = computed(() => {
  const builtin = baseTabs.value.map((tab) => ({ ...tab }))
  const panels = sidebarPanels.value.map((entry) => ({
    key: `panel:${entry.pluginId}:${entry.panel.id}`,
    label: entry.panel.title,
    icon: entry.panel.icon,
    order: entry.panel.order ?? 600,
    entry,
  }))
  return [...builtin, ...panels].sort((a, b) => a.order - b.order)
})

function isActive(item: { key: string }): boolean {
  if (item.key.startsWith('panel:')) {
    const v = activeView.value
    // key = panel:{pluginId}:{panel.id}，与打开面板时存入的 _panelId 比对
    return v?.kind === 'sidebar' && `${v.pluginId}:${(v as any)._panelId}` === item.key.slice(6)
  }
  return activeTab.value === item.key
}

function selectSidebar(item: { key: string; entry?: any }) {
  if (item.key.startsWith('panel:') && item.entry) {
    openActiveView({
      kind: 'sidebar',
      pluginId: item.entry.pluginId,
      title: item.entry.panel.title,
      component: item.entry.panel.component,
      _panelId: item.entry.panel.id,
    })
    return
  }
  openActiveView(null)
  activeTab.value = item.key as BaseTab
}

function backHome() {
  openActiveView(null)
  activeTab.value = 'toolbox'
}

const clock = ref('')
setInterval(() => {
  clock.value = new Date().toLocaleTimeString('zh-CN', { hour12: false })
}, 30_000)
clock.value = new Date().toLocaleTimeString('zh-CN', { hour12: false })

window.addEventListener('beforeunload', () => {
  void deactivateAll()
})
</script>

<template>
  <div class="desktop-ui flex flex-col bg-page text-[var(--text-primary)]">
    <!-- 标题栏（与宿主 TitleBar 同构：40px、logo SVG 随主题切换、bg-card） -->
    <header
      class="h-10 flex-shrink-0 flex items-center gap-3 px-4 border-b border-[var(--border)] bg-[var(--bg-card)] select-none"
    >
      <!-- 品牌图标：与宿主 TitleBar 同一份 logo SVG，填充色随 light/dark 主题切换 -->
      <svg
        class="w-5 h-5 flex-shrink-0 [--logo-bg-start:#2E2A22] [--logo-bg-end:#0A0907] [--logo-fg:#FFFFFF] dark:[--logo-bg-start:#FAF9F7] dark:[--logo-bg-end:#E7E4DC] dark:[--logo-fg:#1C1917]"
        viewBox="0 0 100 100"
        aria-hidden="true"
      >
        <defs>
          <linearGradient id="titlebar-logo-bg" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="var(--logo-bg-start)" />
            <stop offset="100%" stop-color="var(--logo-bg-end)" />
          </linearGradient>
        </defs>
        <rect width="100" height="100" rx="18" fill="url(#titlebar-logo-bg)" />
        <path d="M 24 18 L 59 50 L 24 82 L 32 74 L 51 50 L 32 26 Z" fill="var(--logo-fg)" />
        <path d="M 51 60 L 84 62 L 53 65 Z" fill="var(--logo-fg)" />
      </svg>
      <span
        class="text-[calc(13px*var(--ui-scale))] font-semibold tracking-tight text-[var(--text-primary)] whitespace-nowrap"
        >{{ t('devshell.brand') }}</span
      >
      <span v-if="plugins.length" class="text-xs text-[var(--text-tertiary)] truncate min-w-0">
        {{ pluginSummary }}
      </span>
      <span class="flex-1" />
      <button
        v-for="entry in titleBarItems"
        :key="entry.pluginId + entry.item.id"
        class="flex items-center gap-1 h-6 px-2 shrink-0 whitespace-nowrap rounded-[6px] text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
        :title="entry.item.label"
        @click="entry.item.onClick?.()"
      >
        <span v-if="entry.item.icon" class="flex-shrink-0">{{ entry.item.icon }}</span>
        {{ entry.item.label }}
      </button>
      <!-- 语言切换（中 / EN 分段按钮） -->
      <div class="flex items-center rounded-btn bg-[var(--bg-hover)] p-0.5">
        <button
          v-for="opt in localeOptions"
          :key="opt.value"
          class="px-2 py-0.5 rounded text-xs transition-colors duration-200"
          :class="
            locale === opt.value
              ? 'bg-[var(--color-primary)]/10 text-[var(--color-primary)]'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
          @click="setLocale(opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
      <button
        class="px-2.5 py-1 rounded-btn text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200"
        @click="fireDemoToasts"
      >
        🍞 Toast 演示
      </button>
      <button
        class="px-2.5 py-1 rounded-btn text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200"
        @click="logOpen = !logOpen"
      >
        {{ t('devshell.logs.title') }}
      </button>
    </header>

    <div class="flex flex-1 min-h-0">
      <!-- 侧边栏（与宿主 Sidebar 同构：导航分组 + 折叠/展开；240px ↔ 56px） -->
      <aside
        class="bg-[var(--bg-sidebar)] flex flex-col border-r border-[var(--border)] flex-shrink-0 relative"
        :style="{ width: collapsed ? `${COLLAPSED_WIDTH}px` : 'var(--sidebar-width)' }"
        :class="!collapsed && 'transition-[width] duration-200 ease'"
      >
        <nav class="flex-1 py-4 overflow-y-auto overflow-x-hidden px-3">
          <h4 v-if="!collapsed" class="wb-sidebar-section px-2 mb-2">
            {{ t('devshell.nav.navigation') }}
          </h4>
          <ul class="space-y-0.5">
            <li v-for="item in sidebarItems" :key="item.key">
              <button
                class="w-full flex items-center gap-2.5 h-9 rounded-md transition-colors duration-200"
                :class="[
                  isActive(item)
                    ? 'bg-[var(--bg-card)] font-medium text-[var(--text-primary)] shadow-sm'
                    : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]',
                  collapsed ? 'justify-center px-0' : 'px-2.5',
                ]"
                :title="collapsed ? item.label : undefined"
                @click="selectSidebar(item)"
              >
                <span v-if="isSvgIcon(item.icon)" class="w-4 h-4 flex-shrink-0">
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.75"
                    class="w-4 h-4"
                  >
                    <path :d="item.icon" />
                  </svg>
                </span>
                <span v-else class="text-base leading-none flex-shrink-0">
                  {{ item.icon || '▫️' }}
                </span>
                <span
                  v-if="!collapsed"
                  class="text-[calc(13px*var(--ui-scale))] whitespace-nowrap truncate min-w-0"
                  >{{ item.label }}</span
                >
              </button>
            </li>
          </ul>
        </nav>

        <!-- 底部折叠/展开按钮（与宿主 Sidebar 同构） -->
        <div class="p-2 border-t border-[var(--border)]">
          <div v-if="!collapsed" class="flex justify-end">
            <button
              class="w-7 h-7 flex items-center justify-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
              :title="t('devshell.sidebar.collapse')"
              @click="toggleCollapsed"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="1.75"
                  d="M15 19l-7-7 7-7"
                />
              </svg>
            </button>
          </div>
          <div v-else class="flex justify-center">
            <button
              class="w-7 h-7 flex items-center justify-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
              :title="t('devshell.sidebar.expand')"
              @click="toggleCollapsed"
            >
              <svg class="w-3.5 h-3.5 rotate-180" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="1.75"
                  d="M15 19l-7-7 7-7"
                />
              </svg>
            </button>
          </div>
        </div>
      </aside>

      <!-- 主内容 -->
      <main class="flex-1 min-w-0 flex flex-col">
        <div class="flex-1 min-h-0 overflow-y-auto">
          <!-- 页面切换过渡（.page-* 类定义于 styles/style.css）：面板/页面跳转淡入淡出 -->
          <Transition name="page" mode="out-in">
            <PanelView
              v-if="activeView"
              :key="activeView.pluginId + ':' + activeView.title"
              @back="backHome"
            />
            <TerminalView v-else-if="activeTab === 'terminal'" />
            <TerminalInputRailDemo v-else-if="activeTab === 'rail'" />
            <ToolboxView v-else-if="activeTab === 'toolbox'" />
            <PluginsView v-else-if="activeTab === 'plugins'" />
            <SettingsView v-else />
          </Transition>
        </div>

        <!-- 状态栏 -->
        <footer
          class="h-8 flex-shrink-0 flex items-center gap-3 px-4 border-t border-[var(--border)] bg-sidebar text-xs text-[var(--text-secondary)]"
        >
          <span
            class="w-2 h-2 rounded-full flex-shrink-0"
            :class="connected ? 'bg-[var(--color-primary)]' : 'bg-[var(--text-tertiary)]'"
          />
          <span class="whitespace-nowrap">{{
            connected ? t('devshell.terminal.connected') : t('devshell.terminal.disconnected')
          }}</span>
          <span class="text-[var(--text-tertiary)] whitespace-nowrap">mock-session-1</span>
          <span class="flex-1" />
          <button
            v-for="entry in statusBarItems"
            :key="entry.pluginId + entry.item.id"
            class="px-1.5 py-0.5 rounded-tag text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors duration-200 flex items-center gap-1"
            @click="entry.item.onClick?.()"
          >
            <!-- 图标约定与侧边栏一致：SVG path d 字符串用 <svg> 渲染，emoji/文本直出 -->
            <span v-if="isSvgIcon(entry.item.icon)" class="w-3.5 h-3.5 flex-shrink-0">
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                class="w-3.5 h-3.5"
              >
                <path :d="entry.item.icon" />
              </svg>
            </span>
            <span v-else-if="entry.item.icon" class="flex-shrink-0">{{ entry.item.icon }}</span>
            <span class="whitespace-nowrap">{{ entry.item.label }}</span>
          </button>
          <span class="text-[var(--text-tertiary)] whitespace-nowrap">{{ clock }}</span>
        </footer>
      </main>
    </div>

    <LogPanel v-model:log-open="logOpen" />

    <!-- 宿主同款 Toast 容器（expand 防重叠；主题跟随宿主设置） -->
    <Toaster
      :theme="toasterTheme"
      position="top-center"
      rich-colors
      expand
      :visible-toasts="6"
      :toast-options="toastOptions"
    />
    <!-- 插件全局弹窗（预设/组件两模式、定时关闭、按钮跳转） -->
    <PluginGlobalDialog />
  </div>
</template>
