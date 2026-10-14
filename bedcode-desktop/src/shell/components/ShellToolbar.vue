<template>
  <header
    class="shell-toolbar flex items-center gap-2 px-2 pl-3 select-none flex-shrink-0"
    data-tauri-drag-region
  >
    <!-- 品牌 + 当前位置（role=status 播报当前位置，不播报裸指标——spec-c §2） -->
    <div class="flex items-center gap-2 min-w-0 shrink-0" data-tauri-drag-region>
      <svg
        class="w-[19px] h-[19px] shrink-0 [--logo-bg:#16181C] [--logo-fg:#F5F7F9] [--logo-accent:#FF9E2C] dark:[--logo-bg:#FAF9F7] dark:[--logo-fg:#16181C] dark:[--logo-accent:#C2701A]"
        viewBox="0 0 100 100"
        aria-hidden="true"
      >
        <rect width="100" height="100" rx="22" fill="var(--logo-bg)" />
        <g stroke="var(--logo-fg)" stroke-width="13" stroke-linecap="round" fill="none">
          <line x1="23.2" y1="33" x2="36.8" y2="67" />
          <line x1="39.3" y1="67.1" x2="48.7" y2="46.9" />
          <line x1="51.3" y1="46.9" x2="60.7" y2="67.1" />
          <line x1="63.2" y1="67" x2="76.8" y2="33" />
        </g>
        <path d="M50 36.5 L56.5 43 L50 49.5 L43.5 43 Z" fill="var(--logo-accent)" />
      </svg>
      <b
        class="text-[calc(13px*var(--ui-scale))] font-semibold tracking-tight text-[var(--text-primary)]"
        >BedCode</b
      >
      <span
        class="tb-where text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] whitespace-nowrap overflow-hidden text-ellipsis min-w-0"
        role="status"
        aria-atomic="true"
      >
        {{ where }}
      </span>
    </div>

    <!-- 导航（可在设置隐藏；Esc 始终等效返回——spec-c §6） -->
    <nav v-if="prefs.showNav" class="flex items-center gap-0.5 ml-2.5 shrink-0" aria-label="导航">
      <button
        class="tb-btn"
        :disabled="!canBack"
        :title="t('shell.nav.backTitle')"
        :aria-label="t('shell.nav.back')"
        @click="state.goBack()"
      >
        <ShellIcon name="back" :size="15" />
        {{ t('shell.nav.back') }}
      </button>
      <button
        class="tb-btn"
        :class="{ on: state.screen.value === 'home' }"
        :title="t('shell.nav.homeTitle')"
        :aria-label="t('shell.nav.home')"
        @click="state.goHome()"
      >
        <ShellIcon name="home" :size="15" />
        {{ t('shell.nav.home') }}
      </button>
    </nav>

    <!-- 全局入口 + 窗口控制 -->
    <div class="flex items-center gap-0.5 ml-auto shrink-0">
      <button
        class="tb-btn"
        :title="t('shell.toolbar.paletteTitle')"
        :aria-label="t('shell.toolbar.palette')"
        @click="togglePalette"
      >
        <ShellIcon name="search" :size="14" />
      </button>
      <button
        class="tb-btn"
        :class="{ on: state.panelOpen.value }"
        data-shell-panel-trigger
        :title="t('shell.toolbar.notifsTitle', { count: state.notifCountTotal.value })"
        :aria-label="t('shell.toolbar.notifs', { count: state.notifCountTotal.value })"
        @click="state.panelOpen.value = !state.panelOpen.value"
      >
        <ShellIcon name="bell" :size="14" />
        <span v-if="state.notifCountTotal.value > 0" class="bdg">
          {{ state.notifCountTotal.value }}
        </span>
      </button>
      <button
        class="tb-btn"
        :title="t('shell.toolbar.themeTitle')"
        :aria-label="t('shell.toolbar.theme')"
        @click="toggleTheme"
      >
        <ShellIcon name="theme" :size="14" />
      </button>
      <span class="tb-sep mx-[5px]" aria-hidden="true"></span>
      <button
        class="tb-win"
        :title="t('shell.toolbar.minimize')"
        :aria-label="t('shell.toolbar.minimize')"
        @click="minimize"
      >
        <ShellIcon name="minus" :size="13" />
      </button>
      <button
        class="tb-win"
        :title="t('shell.toolbar.maximize')"
        :aria-label="t('shell.toolbar.maximize')"
        @click="toggleMaximize"
      >
        <ShellIcon name="square" :size="11" />
      </button>
      <button
        class="tb-win close"
        :title="t('shell.toolbar.close')"
        :aria-label="t('shell.toolbar.close')"
        @click="requestClose"
      >
        <ShellIcon name="x" :size="13" />
      </button>
    </div>
  </header>
</template>

<script setup lang="ts">
/**
 * ShellToolbar — 壳顶部工具栏
 *
 * 结构（spec-c §1 布局）：品牌 · 当前位置 │ 返回/主屏（可隐藏）│ 命令面板 · 通知 · 主题 │ 窗口控制。
 * 窗口控制与 TitleBar 同源（getCurrentWindow）；关闭走退出确认弹窗（壳内 dialog）。
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useSettingsStore } from '@/stores/settings'
import { useTheme } from '@/composables/useTheme'
import { useShellState } from '../useShellState'
import { useShellSettings } from '../useShellSettings'
import ShellIcon from './ShellIcon.vue'

const { t } = useI18n()
const state = useShellState()
const { prefs } = useShellSettings()
const settingsStore = useSettingsStore()
const { isSystemDark } = useTheme()

// 窗口控制：Tauri 环境取真窗口；浏览器预览（无 __TAURI_INTERNALS__）时降级空操作，
// 工具栏不因窗口 API 缺失而整体挂掉（TitleBar 同源 API，此处显式兜底）
const appWindow = (() => {
  try {
    return getCurrentWindow()
  } catch {
    return null
  }
})()
const isMaximized = ref(false)

/** 当前所在位置（role=status 播报） */
const where = computed(() =>
  state.screen.value === 'card' && state.activeCard.value
    ? state.cardTitleOf(state.activeCard.value)
    : t('shell.home.where'),
)

/** 有可收的覆层或不在主屏 → 返回可用 */
const canBack = computed(
  () =>
    state.screen.value !== 'home' ||
    state.dialog.value !== null ||
    state.paletteOpen.value ||
    state.ctxMenu.value !== null ||
    state.panelOpen.value,
)

/** 当前生效暗色（system 跟随系统偏好） */
const isDark = computed(() =>
  settingsStore.settings.ui.theme === 'dark'
    ? true
    : settingsStore.settings.ui.theme === 'light'
      ? false
      : isSystemDark.value,
)

function toggleTheme() {
  settingsStore.settings.ui.theme = isDark.value ? 'light' : 'dark'
  // 保存走整表回传（settings store 已有编排）；useTheme watch 会即时应用
  void settingsStore.saveSettings({ ui: settingsStore.settings.ui })
}

function togglePalette() {
  state.paletteOpen.value = !state.paletteOpen.value
  if (state.paletteOpen.value) state.panelOpen.value = false
}

async function checkMaximized() {
  if (!appWindow) return
  try {
    isMaximized.value = await appWindow.isMaximized()
  } catch {
    /* 非 Tauri 环境（浏览器预览）时静默 */
  }
}

async function minimize() {
  if (!appWindow) return
  try {
    await appWindow.minimize()
  } catch {
    /* 预览环境忽略 */
  }
}

async function toggleMaximize() {
  if (!appWindow) return
  try {
    await appWindow.toggleMaximize()
    setTimeout(checkMaximized, 100)
  } catch {
    /* 预览环境忽略 */
  }
}

/** 关窗：走退出确认弹窗（有运行中应用时给确认） */
function requestClose() {
  state.dialog.value = 'exit'
}

onMounted(checkMaximized)
window.addEventListener('resize', checkMaximized)
onUnmounted(() => window.removeEventListener('resize', checkMaximized))
</script>

<style scoped>
.shell-toolbar {
  height: var(--toolbar-h);
  background: var(--bg-card);
  border-bottom: 1px solid var(--border);
  position: relative;
  z-index: 30;
  -webkit-app-region: drag;
}
.tb-btn,
.tb-win {
  -webkit-app-region: no-drag;
}
.tb-btn {
  min-width: 32px;
  height: 28px;
  padding: 0 7px;
  border-radius: var(--radius-button);
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  font-size: calc(12px * var(--ui-scale));
  position: relative;
  transition:
    background-color 0.15s,
    color 0.15s;
}
.tb-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.tb-btn:disabled {
  opacity: 0.35;
  cursor: default;
}
.tb-btn:disabled:hover {
  background: none;
  color: var(--text-secondary);
}
.tb-btn.on {
  background: var(--color-primary-light);
  color: var(--text-primary);
}
.tb-btn .bdg {
  position: absolute;
  top: 0;
  right: 0;
  min-width: 13px;
  height: 13px;
  padding: 0 3px;
  border-radius: 7px;
  background: var(--color-danger);
  color: var(--color-primary-contrast);
  font-size: calc(9px * var(--ui-scale));
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
}
.tb-sep {
  width: 1px;
  height: 16px;
  background: var(--border-strong);
  flex-shrink: 0;
}
.tb-win {
  width: 34px;
  height: 28px;
  border-radius: var(--radius-button);
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition:
    background-color 0.15s,
    color 0.15s;
  -webkit-app-region: no-drag;
}
.tb-win:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.tb-win.close:hover {
  background: var(--color-danger);
  color: var(--color-primary-contrast);
}
</style>
