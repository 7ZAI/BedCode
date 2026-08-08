<script setup lang="ts">
/**
 * AppShell（桌面端）— 桌面页面骨架
 *
 * 标题栏（+ 插件 titleBar 项）→ 侧边栏（内置导航 + 插件 sidebar 面板，按 order 排序）
 * → 主内容区（activeView 或当前 Tab）→ 状态栏（连接状态 + 插件 statusBar 项）。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
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
import PanelView from './views/PanelView.vue'
import ToolboxView from './views/ToolboxView.vue'
import PluginsView from './views/PluginsView.vue'
import TerminalView from './views/TerminalView.vue'
import SettingsView from './views/SettingsView.vue'
import LogPanel from './components/LogPanel.vue'
import PromptHost from './components/PromptHost.vue'

type BaseTab = 'terminal' | 'toolbox' | 'plugins' | 'settings'
const activeTab = ref<BaseTab>('toolbox')
const logOpen = ref(false)

const { t } = useI18n()

const baseTabs = computed(() => [
  { key: 'terminal' as const, label: t('devshell.nav.terminal'), icon: '⌨️' },
  { key: 'toolbox' as const, label: t('devshell.nav.toolbox'), icon: '🧰' },
  { key: 'plugins' as const, label: t('devshell.nav.plugins'), icon: '🧩' },
  { key: 'settings' as const, label: t('devshell.nav.settings'), icon: '⚙️' },
])

const sidebarItems = computed(() => {
  const builtin = baseTabs.value.map((tab) => ({ ...tab, order: 100 + baseTabs.value.indexOf(tab) }))
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
    return (
      v?.kind === 'sidebar' &&
      `${v.pluginId}:${(v as any)._panelId}` === item.key.slice(6)
    )
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
    <!-- 标题栏 -->
    <header
      class="h-12 flex-shrink-0 flex items-center gap-3 px-4 border-b border-[var(--border)] bg-sidebar"
    >
      <span class="w-3 h-3 rounded-full bg-brand flex-shrink-0" />
      <span class="text-sm font-semibold text-[var(--text-primary)] whitespace-nowrap">{{ t('devshell.brand') }}</span>
      <span v-if="plugins.length" class="text-xs text-[var(--text-tertiary)] truncate min-w-0">
        {{ plugins.map((p) => p.name).join('，') }}
      </span>
      <span class="flex-1" />
      <button
        v-for="entry in titleBarItems"
        :key="entry.pluginId + entry.item.id"
        class="px-2.5 py-1 rounded-btn text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200 whitespace-nowrap"
        @click="entry.item.onClick?.()"
      >
        {{ entry.item.icon ? entry.item.icon + ' ' : '' }}{{ entry.item.label }}
      </button>
      <button
        class="px-2.5 py-1 rounded-btn text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200"
        @click="logOpen = !logOpen"
      >
        {{ t('devshell.logs.title') }}
      </button>
    </header>

    <div class="flex flex-1 min-h-0">
      <!-- 侧边栏 -->
      <aside
        class="w-[var(--sidebar-width)] flex-shrink-0 bg-sidebar border-r border-[var(--border)] overflow-y-auto p-2"
      >
        <button
          v-for="item in sidebarItems"
          :key="item.key"
          class="w-full flex items-center gap-2.5 px-3 py-2 rounded-nav text-sm text-left transition-colors duration-200"
          :class="
            isActive(item)
              ? 'bg-[var(--color-primary)]/10 text-[var(--color-primary)] font-medium'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
          "
          @click="selectSidebar(item)"
        >
          <span v-if="isSvgIcon(item.icon)" class="w-4 h-4 flex-shrink-0">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4">
              <path :d="item.icon" />
            </svg>
          </span>
          <span v-else class="text-base leading-none flex-shrink-0">{{ item.icon || '▫️' }}</span>
          <span class="truncate min-w-0">{{ item.label }}</span>
        </button>
      </aside>

      <!-- 主内容 -->
      <main class="flex-1 min-w-0 flex flex-col">
        <div class="flex-1 min-h-0 overflow-y-auto">
          <PanelView v-if="activeView" @back="backHome" />
          <TerminalView v-else-if="activeTab === 'terminal'" />
          <ToolboxView v-else-if="activeTab === 'toolbox'" />
          <PluginsView v-else-if="activeTab === 'plugins'" />
          <SettingsView v-else />
        </div>

        <!-- 状态栏 -->
        <footer
          class="h-8 flex-shrink-0 flex items-center gap-3 px-4 border-t border-[var(--border)] bg-sidebar text-xs text-[var(--text-secondary)]"
        >
          <span
            class="w-2 h-2 rounded-full flex-shrink-0"
            :class="connected ? 'bg-[var(--color-primary)]' : 'bg-[var(--text-tertiary)]'"
          />
          <span class="whitespace-nowrap">{{ connected ? t('devshell.terminal.connected') : t('devshell.terminal.disconnected') }}</span>
          <span class="text-[var(--text-tertiary)] whitespace-nowrap">mock-session-1</span>
          <span class="flex-1" />
          <button
            v-for="entry in statusBarItems"
            :key="entry.pluginId + entry.item.id"
            class="px-1.5 py-0.5 rounded-tag text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors duration-200"
            @click="entry.item.onClick?.()"
          >
            {{ entry.item.icon ? entry.item.icon + ' ' : '' }}{{ entry.item.label }}
          </button>
          <span class="text-[var(--text-tertiary)] whitespace-nowrap">{{ clock }}</span>
        </footer>
      </main>
    </div>

    <LogPanel v-model:log-open="logOpen" />
    <PromptHost />
  </div>
</template>
