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

// 内置 tab 图标：与宿主 MobileNav 一致的内联 SVG 线性描边（stroke-width 2），
// 不用 emoji（宿主无 emoji 导航，保证预览与真机一致）
const baseTabs = computed(() => [
  { key: 'terminal' as const, label: t('devshell.nav.terminal'), icon: 'M4 17l6-5-6-5m8 10h8' },
  { key: 'toolbox' as const, label: t('devshell.nav.toolbox'), icon: 'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z' },
  { key: 'plugins' as const, label: t('devshell.nav.plugins'), icon: 'M19.439 7.85c-.049.322.059.648.289.878l1.568 1.568c.47.47.706 1.087.706 1.704s-.235 1.233-.706 1.704l-1.611 1.611a.98.98 0 01-.837.276c-.47-.07-.802-.48-.968-.925a2.501 2.501 0 10-3.214 3.214c.446.166.855.497.925.968a.979.979 0 01-.276.837l-1.61 1.61a2.404 2.404 0 01-1.705.707 2.402 2.402 0 01-1.704-.706l-1.568-1.568a1.026 1.026 0 00-.877-.29c-.493.074-.84.504-1.02.968a2.5 2.5 0 11-3.237-3.237c.464-.18.894-.527.967-1.02a1.026 1.026 0 00-.289-.877l-1.568-1.568A2.402 2.402 0 011.841 11.7a2.402 2.402 0 01.706-1.704l1.611-1.61a.98.98 0 01.837-.277c.47.07.802.48.968.925a2.501 2.501 0 103.214-3.214c-.446-.166-.855-.497-.925-.968a.979.979 0 01.276-.837l1.61-1.61c.454-.454 1.068-.706 1.704-.706.636 0 1.25.252 1.705.706z' },
])

/** 内置 tab 激活判定：插件工具箱页打开时工具箱 tab 保持高亮（导航归属不变） */
function isBaseTabActive(key: BaseTab): boolean {
  if (activeTab.value === key && !activeView.value) return true
  return key === 'toolbox' && activeView.value?.kind === 'toolbox'
}

const pageTitle = computed(() => {
  if (activeView.value) return activeView.value.title || ''
  return baseTabs.value.find((tab) => tab.key === activeTab.value)?.label || ''
})

/**
 * 全局页头显隐：
 * - 无插件视图：显示（当前 tab 名）
 * - toolbox / navTab 视图（header:false，插件无自渲染页头）：由本页头接管 back + 标题
 * - route 视图：插件自渲染页头（SettingsPage 自带 header），不再叠加全局页头，避免双标题
 */
const showGlobalHeader = computed(
  () => !activeView.value || (activeView.value.header === false && activeView.value.kind !== 'route'),
)

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

    <!-- 页头：仅当无插件视图或插件视图声明 header:false（由本页头接管）时显示；
         route 视图插件自渲染页头时隐藏，避免双标题 -->
    <div
      v-if="showGlobalHeader"
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
        class="relative flex-1 min-w-0 flex flex-col items-center justify-center gap-0.5 py-2 text-[11px] transition-colors duration-200"
        :class="isBaseTabActive(tab.key) ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-muted)]'"
        @click="switchTab(tab.key)"
      >
        <span v-if="isSvgIcon(tab.icon)" class="w-5 h-5 flex items-center justify-center">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-5 h-5">
            <path :d="tab.icon" />
          </svg>
        </span>
        <span v-else class="text-base leading-none">{{ tab.icon || '🧩' }}</span>
        <span class="truncate max-w-full">{{ tab.label }}</span>
        <span
          v-if="isBaseTabActive(tab.key)"
          class="absolute bottom-0.5 left-1/2 -translate-x-1/2 w-4 h-0.5 rounded-full bg-[var(--mobile-accent)]"
        ></span>
      </button>
      <button
        v-for="entry in navTabs"
        :key="entry.pluginId + entry.tab.id"
        class="relative flex-1 min-w-0 flex flex-col items-center justify-center gap-0.5 py-2 text-[11px] transition-colors duration-200"
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
        <span
          v-if="isNavTabActive(entry.pluginId, entry.tab.id)"
          class="absolute bottom-0.5 left-1/2 -translate-x-1/2 w-4 h-0.5 rounded-full bg-[var(--mobile-accent)]"
        ></span>
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
