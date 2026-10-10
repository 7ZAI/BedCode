<template>
  <div class="app-root flex flex-col h-full min-h-0">
    <!-- 终端子页：整页沉浸（自带 Header / 输入栏 / 键盘避让），不进页签容器也不带导航 -->
    <TerminalView v-if="terminalSession" :session-id="terminalSession.id" :back="closeTerminal" />

    <template v-else>
      <!-- ==================== 头部：标题 + 连接状态 + 刷新 ==================== -->
      <div class="flex items-center gap-2 px-4 pt-3 pb-2 flex-shrink-0">
        <span class="text-base font-semibold" :style="{ color: 'var(--mobile-text-primary)' }">
          {{ t('app.title') }}
        </span>
        <span class="text-xs px-2 py-0.5 rounded-full shrink-0" :style="statusChipStyle">
          {{ t(statusTextKey) }}
        </span>
        <div class="flex-1" />
        <button
          v-if="isAuthenticated"
          type="button"
          class="flex items-center justify-center min-w-[36px] min-h-[36px] rounded-lg transition-colors active:opacity-70"
          :style="{ color: 'var(--mobile-text-secondary)' }"
          :aria-label="t('app.refresh')"
          @click="onRefreshSessions"
        >
          <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
      </div>

      <!-- 重连横幅（引擎事实：重连中计数） -->
      <p
        v-if="reconnectBanner"
        class="mx-4 mb-1 rounded-lg px-3 py-1.5 text-xs flex-shrink-0"
        :style="{ background: 'var(--mobile-bg-card)', color: 'var(--mobile-text-secondary)' }"
      >
        {{ t('app.reconnecting') }}
      </p>

      <!-- ==================== 页签容器：首次访问后常驻，横滑或点导航切换 ==================== -->
      <div
        class="relative flex-1 min-h-0"
        @touchstart.passive="onZoneTouchStart"
        @touchmove.passive="onZoneTouchMove"
        @touchend="onZoneTouchEnd"
        @touchcancel="onZoneTouchEnd"
      >
        <div
          v-if="isVisited('connection')"
          :class="['app-tab-panel absolute inset-0 overflow-y-auto', panelClass('connection')]"
        >
          <DevicesSection @open-sessions="onSelectTab('sessions')" />
        </div>
        <div
          v-if="isVisited('sessions')"
          :class="['app-tab-panel absolute inset-0 overflow-y-auto', panelClass('sessions')]"
        >
          <SessionsSection @open-terminal="onOpenTerminal" />
        </div>
        <div
          v-if="isVisited('toolbox')"
          :class="['app-tab-panel absolute inset-0', panelClass('toolbox')]"
        >
          <AutoTaskToolboxView />
        </div>
        <div
          v-if="isVisited('settings')"
          :class="['app-tab-panel absolute inset-0 overflow-y-auto', panelClass('settings')]"
        >
          <SettingsPage />
        </div>
      </div>

      <!-- ==================== 底部导航 ==================== -->
      <NavBar :tabs="tabs.tabs.value" :active="tabs.active.value" @select="onSelectTab" />
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 应用运行面（票 2026-10-10：全量 UI 下沉 —— 承接旧宿主主流程整体页）
 *
 * 本组件是 terminal-session wasm-app 在宿主壳内的运行面（壳 /mobile/shell →
 * 应用运行屏 → 本组件）。它承接旧宿主 `MobileLayout + MobileSwipeContainer +
 * MobileNav` 三件套的职责：
 * - 头部（标题 / 连接状态 / 刷新）与页签容器在本组件
 * - 底部导航见 ./components/NavBar.vue（复刻旧宿主 MobileNav）
 * - 横滑翻页经 SDK useSwipeTabs；嵌套区（工具箱双页签）越界时经
 *   ./swipeArbitration 上交本组件——旧宿主由 MobileSwipeContainer 承担的仲裁职责
 *
 * 终端为整页沉浸态：进终端后隐藏头部与导航（终端自带 Header / 输入栏 / 键盘避让）。
 * 数据面全部走 useHostPage（宿主投影 + 插件命令），本组件不直连宿主 Tauri 命令。
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useSwipeTabs } from '@binblink/bedcode-plugin-sdk-mobile/ui/swipe-tabs'
import DevicesSection from '../host/components/DevicesSection.vue'
import SessionsSection from '../host/components/SessionsSection.vue'
import { useHostPage } from '../host/useHostPage'
import SettingsPage from '../settings/SettingsPage.vue'
import AutoTaskToolboxView from '../task/components/AutoTaskToolboxView.vue'
import TerminalView from '../terminal/TerminalView.vue'
import NavBar from './components/NavBar.vue'
import { setSwipeDelegate } from './swipeArbitration'
import { useAppTabs, type AppTab } from './useAppTabs'
import './app.css'

/** 已具备内容的页签（顺序即 AVAILABLE_TABS 的数组序，仅作面板停靠的相对序依据） */
const AVAILABLE_TABS: readonly AppTab[] = ['connection', 'sessions', 'toolbox', 'settings']

const ctrl = useHostPage()
const t = ctrl.t
const logger = ctrl.logger

const tabs = useAppTabs(AVAILABLE_TABS)
const terminalSession = ref<{ id: string; name: string } | null>(null)

const isAuthenticated = ctrl.isAuthenticated
const reconnectBanner = ctrl.reconnectBanner

/** 连接状态 → 文案键（复用 host 域状态分类表，仅换 app.* 命名空间的落点） */
const statusTextKey = computed(() => {
  switch (ctrl.connectionStatus.value) {
    case 'disconnected':
      return 'app.status.disconnected'
    case 'connecting':
      return 'app.status.connecting'
    case 'connected':
      return 'app.status.connected'
    case 'pairing':
      return 'app.status.pairing'
    case 'paired':
      return 'app.status.paired'
    case 'error':
      return 'app.status.error'
    default:
      return 'app.status.unknown'
  }
})

const statusChipStyle = computed(() => {
  const status = ctrl.connectionStatus.value
  const ok = status === 'paired' || status === 'connected'
  return {
    background: ok ? 'var(--mobile-accent-muted)' : 'var(--mobile-bg-card)',
    color: ok ? 'var(--mobile-accent)' : 'var(--mobile-text-secondary)',
  }
})

/**
 * 面板停靠态：当前页居中；已过页停左侧、未到页停右侧
 */
function panelClass(id: AppTab): string {
  const order = AVAILABLE_TABS
  const current = order.indexOf(tabs.active.value)
  const index = order.indexOf(id)
  if (index === current) return 'app-tab-panel--active'
  return index < current ? 'app-tab-panel--left' : 'app-tab-panel--right'
}

/**
 * 首次访问后才挂载，挂过则常驻
 *
 * 不做「打开即全挂」：工具箱页挂载即建立 WS 订阅并发轮询，连接/设置页挂载即读 KV，
 * 用户没进过的页签不必付这份开销。访问过一次后不再卸载，切页保留滚动位置与订阅
 * （旧宿主 MobileSwipeContainer 的常驻语义在此等价实现）。
 */
const visited = ref<Set<AppTab>>(new Set([tabs.active.value]))

function isVisited(id: AppTab): boolean {
  return visited.value.has(id)
}

function markVisited(id: AppTab): void {
  if (visited.value.has(id)) return
  visited.value = new Set(visited.value).add(id)
}

function onSelectTab(id: AppTab): void {
  if (!tabs.select(id)) return
  markVisited(id)
}

function stepTab(dir: 'left' | 'right'): void {
  if (!tabs.step(dir)) return
  markVisited(tabs.active.value)
}

// ==================== 横滑翻页 + 嵌套区仲裁 ====================
//
// 触摸起点落在嵌套横滑区（工具箱双页签声明 data-swipe-zone）内时，本容器整轮
// 不判定手势，交内层处理；内层到边界再经 delegateSwipe 上交回来翻页。

const {
  onTouchStart: onZoneTouchStart,
  onTouchMove: onZoneTouchMove,
  onTouchEnd: onZoneTouchEnd,
} = useSwipeTabs(
  (dir) => {
    stepTab(dir)
  },
  {
    shouldSkip: (target) => !!(target as HTMLElement | null)?.closest?.('[data-swipe-zone]'),
  },
)

let subscriptions: { dispose(): void }[] = []
onMounted(() => {
  subscriptions = ctrl.subscribe()
  setSwipeDelegate((dir) => {
    stepTab(dir)
  })
  // 首屏预取：失败不阻塞页面，但必须可观测（禁止静默 catch）
  void ctrl
    .loadHistory(true)
    .catch((e) => logger.warn(`[app] load connection history failed: ${e instanceof Error ? e.message : e}`))
  void ctrl.refreshBiometric()
  // 会话数据：已认证时拉起一次（配对成功事件也会触发页面级刷新）
  if (ctrl.isAuthenticated.value) void ctrl.refreshSessions()
})

onUnmounted(() => {
  for (const d of subscriptions) d.dispose()
  subscriptions = []
  setSwipeDelegate(null)
  terminalSession.value = null
})

function onOpenTerminal(sessionId: string, sessionName: string): void {
  terminalSession.value = { id: sessionId, name: sessionName }
}

function closeTerminal(): void {
  terminalSession.value = null
}

async function onRefreshSessions(): Promise<void> {
  try {
    await ctrl.refreshSessions()
  } catch (e) {
    logger.error(`[app] refresh sessions failed: ${e instanceof Error ? e.message : e}`)
    ctrl.toast.showToast(t('hub.loadFailed'), 'error')
  }
}
</script>