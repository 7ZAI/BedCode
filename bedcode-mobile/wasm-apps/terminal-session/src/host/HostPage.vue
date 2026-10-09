<template>
  <div class="host-page flex flex-col h-full min-h-0">
    <!-- 终端子页：整页沉浸（自带 Header / 输入栏 / 键盘避让） -->
    <TerminalView v-if="terminalSession" :session-id="terminalSession.id" :back="closeTerminal" />

    <template v-else>
      <!-- ==================== 头部：标题 + 连接状态 + 刷新 ==================== -->
      <div class="flex items-center gap-2 px-4 pt-3 pb-2">
        <span class="text-base font-semibold" :style="{ color: 'var(--mobile-text-primary)' }">
          {{ t('hub.title') }}
        </span>
        <span
          class="text-xs px-2 py-0.5 rounded-full shrink-0"
          :style="statusChipStyle"
        >
          {{ statusText }}
        </span>
        <div class="flex-1" />
        <button
          v-if="isAuthenticated"
          type="button"
          class="flex items-center justify-center min-w-[36px] min-h-[36px] rounded-lg transition-colors active:opacity-70"
          :style="{ color: 'var(--mobile-text-secondary)' }"
          :aria-label="t('hub.refresh')"
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
        class="mx-4 mb-1 rounded-lg px-3 py-1.5 text-xs"
        :style="{ background: 'var(--mobile-bg-card)', color: 'var(--mobile-text-secondary)' }"
      >
        {{ t('hub.status.connecting') }}
      </p>

      <!-- ==================== 分段切换：设备 / 会话 ==================== -->
      <div class="flex gap-1 px-4 pb-2">
        <button
          v-for="seg in segments"
          :key="seg.id"
          type="button"
          class="flex-1 min-h-[38px] rounded-lg text-sm font-medium transition-colors"
          :style="segmentStyle(seg.id)"
          @click="view = seg.id"
        >
          {{ seg.label }}
        </button>
      </div>

      <!-- ==================== 内容区 ==================== -->
      <div class="flex-1 min-h-0 overflow-y-auto">
        <DevicesSection v-if="view === 'devices'" @open-sessions="view = 'sessions'" />
        <SessionsSection v-else @open-terminal="onOpenTerminal" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 宿主页（票 2026-10-09：旧宿主主流程下沉的「整体页面」）
 *
 * 本页是 terminal-session wasm-app 在宿主壳（/mobile/shell）内的运行面：
 *   设备（mDNS 发现 / 手动连接 / 连接历史 / 配对 / 生物）
 *   → 会话（列表 / 起停删 / 从配置启动）
 *   → 终端（复用终端域 TerminalView，back 覆盖为页内返回）
 *
 * 连接态 / 会话 / 配对数据面见 useHostPage；页面内不直连宿主 Tauri 命令。
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import TerminalView from '../terminal/TerminalView.vue'
import DevicesSection from './components/DevicesSection.vue'
import SessionsSection from './components/SessionsSection.vue'
import { useHostPage } from './useHostPage'
import { statusKey } from './utils'

const ctrl = useHostPage()
const t = ctrl.t
const logger = ctrl.logger

type HostView = 'devices' | 'sessions'

const view = ref<HostView>('devices')
const terminalSession = ref<{ id: string; name: string } | null>(null)

const segments = computed(() => [
  { id: 'devices' as const, label: t('hub.devices') },
  { id: 'sessions' as const, label: t('hub.sessions') },
])

const isAuthenticated = ctrl.isAuthenticated
const connectionStatus = ctrl.connectionStatus
const reconnectBanner = ctrl.reconnectBanner

const statusText = computed(() => {
  const key = statusKey(connectionStatus.value)
  return t(key)
})

const statusChipStyle = computed(() => {
  const ok =
    connectionStatus.value === 'paired' || connectionStatus.value === 'connected'
  return {
    background: ok ? 'var(--mobile-accent-muted)' : 'var(--mobile-bg-card)',
    color: ok ? 'var(--mobile-accent)' : 'var(--mobile-text-secondary)',
  }
})

function segmentStyle(seg: HostView) {
  const active = view.value === seg
  return {
    background: active ? 'var(--mobile-accent)' : 'var(--mobile-bg-card)',
    color: active ? 'var(--mobile-text-on-accent)' : 'var(--mobile-text-secondary)',
  }
}

function onOpenTerminal(sessionId: string, sessionName: string) {
  terminalSession.value = { id: sessionId, name: sessionName }
}

function closeTerminal() {
  terminalSession.value = null
}

async function onRefreshSessions() {
  try {
    await ctrl.refreshSessions()
  } catch (e) {
    logger.error(`[host] refresh sessions failed: ${e instanceof Error ? e.message : e}`)
    ctrl.toast.showToast(t('hub.loadFailed'), 'error')
  }
}

let subscriptions: { dispose(): void }[] = []
onMounted(() => {
  subscriptions = ctrl.subscribe()
  // 首屏预取：失败不阻塞页面，但必须可观测（禁止静默 catch）
  void ctrl
    .loadHistory(true)
    .catch((e) => logger.warn(`[host] load connection history failed: ${e instanceof Error ? e.message : e}`))
  void ctrl.refreshBiometric()
  // 会话数据：已认证时拉起一次（配对成功事件也会触发页面级刷新）
  if (ctrl.isAuthenticated.value) void ctrl.refreshSessions()
})
onUnmounted(() => {
  for (const d of subscriptions) d.dispose()
  subscriptions = []
  terminalSession.value = null
})
</script>