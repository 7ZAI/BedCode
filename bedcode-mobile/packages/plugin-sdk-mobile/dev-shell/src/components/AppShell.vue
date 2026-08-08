<script setup lang="ts">
/**
 * AppShell — 移动端页面骨架（与宿主一致的结构与 token）
 *
 * 状态栏 → 页头 → 内容区 → 底部导航；底部导航 = 内置三项 + 插件 navTab 注册项。
 * 插件工具箱页/路由/设置区经 activeView 在内容区渲染（PluginView）。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { activeView, goBackView, navTabs, openActiveView, plugins } from '../registry'
import { deactivateAll } from '../loader'
import PluginView from '../views/PluginView.vue'
import MockTerminalView from '../views/MockTerminalView.vue'
import ToolboxView from '../views/ToolboxView.vue'
import PluginsView from '../views/PluginsView.vue'
import { isSvgIcon } from '../utils/icon'

type BaseTab = 'terminal' | 'toolbox' | 'plugins'
const activeTab = ref<BaseTab>('toolbox')
const clock = ref('')

const { t } = useI18n()

const baseTabs = computed(() => [
  { key: 'terminal' as const, label: t('devshell.nav.terminal'), icon: '⌨️' },
  { key: 'toolbox' as const, label: t('devshell.nav.toolbox'), icon: '🧰' },
  { key: 'plugins' as const, label: t('devshell.nav.plugins'), icon: '🧩' },
])

const pageTitle = computed(() => {
  if (activeView.value) return activeView.value.title || ''
  return baseTabs.value.find((tab) => tab.key === activeTab.value)?.label || ''
})

/** 底部导航切换：先关闭打开的插件视图 */
function switchTab(tab: BaseTab) {
  openActiveView(null)
  activeTab.value = tab
}

function openNavTab(pluginId: string, tabId: string) {
  const entry = navTabs.value.find((n) => n.pluginId === pluginId && n.tab.id === tabId)
  if (!entry) return
  // 再次点击已打开的 navTab → 关闭
  if (
    activeView.value?.kind === 'navtab' &&
    activeView.value.pluginId === pluginId &&
    (activeView.value as any)._tabId === tabId
  ) {
    openActiveView(null)
    return
  }
  openActiveView({
    kind: 'navtab',
    pluginId,
    title: entry.tab.title,
    component: entry.tab.component,
    header: false,
    _tabId: tabId,
  } as any)
}

function isNavTabActive(pluginId: string, tabId: string): boolean {
  const v = activeView.value
  return v?.kind === 'navtab' && v.pluginId === pluginId && (v as any)._tabId === tabId
}

function tick() {
  clock.value = new Date().toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
}

onMounted(() => {
  tick()
  const timer = setInterval(tick, 30_000)
  window.addEventListener('beforeunload', onBeforeUnload)
  onBeforeUnmount(() => {
    clearInterval(timer)
    window.removeEventListener('beforeunload', onBeforeUnload)
  })
})

function onBeforeUnload() {
  void deactivateAll()
}
</script>

<template>
  <div
    class="mobile-ui mobile-app flex flex-col bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-primary)] min-h-0 overflow-hidden"
    style="transition: background-color 0.2s, color 0.2s, border-color 0.2s"
  >
    <!-- 状态栏（骨架装饰） -->
    <div
      class="h-7 flex-shrink-0 flex items-center justify-between px-5 text-[11px] text-[var(--mobile-text-muted)]"
    >
      <span>{{ clock }}</span>
      <span class="truncate min-w-0 text-[var(--mobile-text-secondary)]">BedCode Dev Shell</span>
      <span class="flex items-center gap-1">📶 🔋</span>
    </div>

    <!-- 页头 -->
    <div
      class="h-11 flex-shrink-0 flex items-center gap-2 px-4 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl"
    >
      <button
        v-if="activeView"
        class="w-8 h-8 flex-shrink-0 flex items-center justify-center rounded-lg text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-text-primary)] transition-colors duration-200"
        aria-label="back"
        @click="goBackView()"
      >
        ←
      </button>
      <h1 class="text-[15px] font-semibold truncate min-w-0">{{ pageTitle }}</h1>
    </div>

    <!-- 内容区 -->
    <div class="flex-1 min-h-0 overflow-y-auto">
      <PluginView v-if="activeView" />
      <MockTerminalView v-else-if="activeTab === 'terminal'" />
      <ToolboxView v-else-if="activeTab === 'toolbox'" />
      <PluginsView v-else />
    </div>

    <!-- 底部导航（内置 + 插件 navTab） -->
    <nav
      class="bottom-nav mobile-nav-safe flex-shrink-0 flex items-stretch border-t border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/95 backdrop-blur-xl"
    >
      <button
        v-for="tab in baseTabs"
        :key="tab.key"
        class="flex-1 min-w-0 flex flex-col items-center justify-center gap-0.5 py-2 text-[11px] transition-colors duration-200"
        :class="
          activeTab === tab.key && !activeView
            ? 'text-[var(--mobile-accent)]'
            : 'text-[var(--mobile-text-muted)]'
        "
        @click="switchTab(tab.key)"
      >
        <span class="text-base leading-none">{{ tab.icon }}</span>
        <span class="truncate max-w-full">{{ tab.label }}</span>
      </button>
      <button
        v-for="entry in navTabs"
        :key="entry.pluginId + entry.tab.id"
        class="flex-1 min-w-0 flex flex-col items-center justify-center gap-0.5 py-2 text-[11px] transition-colors duration-200"
        :class="isNavTabActive(entry.pluginId, entry.tab.id) ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-muted)]'"
        @click="openNavTab(entry.pluginId, entry.tab.id)"
      >
        <span v-if="isSvgIcon(entry.tab.icon)" class="w-5 h-5 flex items-center justify-center">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-5 h-5">
            <path :d="entry.tab.icon" />
          </svg>
        </span>
        <span v-else class="text-base leading-none">{{ entry.tab.icon || '🧩' }}</span>
        <span class="truncate max-w-full">{{ entry.tab.title }}</span>
      </button>
    </nav>

    <!-- 提示：当前调试插件数 -->
    <div
      v-if="plugins.length === 0"
      class="absolute bottom-20 left-0 right-0 flex justify-center pointer-events-none"
    >
      <span class="px-3 py-1 rounded-full text-[11px] bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]">
        未加载插件 — 在插件目录运行 bedcode-plugin dev
      </span>
    </div>
  </div>
</template>
