<template>
  <div class="flex flex-col h-screen desktop-ui bg-page text-[var(--text-primary)]">
    <!-- 键盘导航跳过链接（ui-ux-pro-max --domain ux：nav-heavy 页需 skip link） -->
    <a
      href="#shell-main"
      class="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-[9999] focus:px-3 focus:py-1.5 focus:rounded-btn focus:bg-card focus:border focus:border-[var(--border-strong)] focus:text-[var(--text-primary)]"
    >
      {{ t('shell.skipToContent') }}
    </a>

    <!-- 顶部工具栏 -->
    <ShellToolbar />

    <!-- 舞台：home / card 两屏（同一时刻一屏）+ 通知面板覆层 -->
    <main id="shell-main" class="stage">
      <ShellHome
        v-show="state.screen.value === 'home'"
        :class="{ 'screen-entering': enteringScreen }"
      />
      <ShellCardScreen
        v-show="state.screen.value === 'card'"
        :class="{ 'screen-entering': enteringScreen }"
      />
      <ShellNotificationPanel />
    </main>

    <!-- 全局覆层 -->
    <ShellContextMenu />
    <ShellCommandPalette />
    <ShellDialogLayer />
    <ShellBootScreen />

    <!-- type-to-search 隐藏输入（IME 合成中的 CJK 经真实输入框承接，spec-c 待决策 7） -->
    <input
      ref="searchInput"
      class="type-search-input"
      :value="state.homeQ.value"
      aria-label="主屏搜索"
      @input="onSearchInput"
      @keydown.esc="clearSearch"
      @keydown.enter="openFirstMatch"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * ShellView — 设备壳路由根（方案 C：主屏 + 卡片切换）
 *
 * 两屏模型（home / card）+ 覆层（面板/右键/命令面板/弹窗/启动屏）。
 * 键盘：Esc 逐层收覆层 → 清搜索 → card→home；Ctrl+Shift+P 命令面板；Ctrl+, 设置卡。
 * 主题 / 字号复用既有 useTheme / useFontSize（与旧布局同一设置真源）。
 */
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTheme } from '@/composables/useTheme'
import { useFontSize } from '@/composables/useFontSize'
import { useShellState } from './useShellState'
import { useShellApps } from './useShellApps'
import ShellToolbar from './components/ShellToolbar.vue'
import ShellHome from './components/ShellHome.vue'
import ShellCardScreen from './components/ShellCardScreen.vue'
import ShellNotificationPanel from './components/ShellNotificationPanel.vue'
import ShellContextMenu from './components/ShellContextMenu.vue'
import ShellCommandPalette from './components/ShellCommandPalette.vue'
import ShellDialogLayer from './components/ShellDialogLayer.vue'
import ShellBootScreen from './components/ShellBootScreen.vue'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()

const { setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

/** 切屏动效：进入即加类、动画结束移除（类只是装饰，状态已先行正确——spec-c §1） */
const enteringScreen = ref(false)
let enteringTimer: ReturnType<typeof setTimeout> | null = null
watch(
  () => state.screen.value,
  () => {
    enteringScreen.value = false
    requestAnimationFrame(() => {
      enteringScreen.value = true
    })
    if (enteringTimer) clearTimeout(enteringTimer)
    enteringTimer = setTimeout(() => {
      enteringScreen.value = false
    }, 260)
  },
)

// ==================== type-to-search（隐藏 input） ====================

const searchInput = ref<HTMLInputElement | null>(null)

function onSearchInput(e: Event) {
  state.homeQ.value = (e.target as HTMLInputElement).value
}

function clearSearch() {
  state.homeQ.value = ''
  searchInput.value?.blur()
}

/** Enter：打开首个匹配（应用优先，其次系统页） */
function openFirstMatch() {
  const q = state.homeQ.value.trim().toLowerCase()
  if (!q) return
  const hitApp = appsStore.plugins.value.find(
    (a) => a.name.toLowerCase().includes(q) || a.id.toLowerCase().includes(q),
  )
  const hitSys = ['apps', 'perms', 'settings'].find((id) => id.includes(q))
  state.homeQ.value = ''
  searchInput.value?.blur()
  if (hitApp) void state.openApp(hitApp.id)
  else if (hitSys) state.openSys(hitSys)
}

// ==================== 全局键盘 ====================

function onKeydown(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey

  // 启动屏：任意键跳过
  if (state.boot.value) {
    if (e.key === 'Escape' || e.key === 'Enter') state.boot.value = false
    return
  }

  // Ctrl+Shift+P：命令面板
  if (mod && e.shiftKey && (e.key === 'P' || e.key === 'p')) {
    e.preventDefault()
    state.paletteOpen.value = !state.paletteOpen.value
    if (state.paletteOpen.value) state.panelOpen.value = false
    return
  }
  // Ctrl+,：设置卡
  if (mod && e.key === ',') {
    e.preventDefault()
    state.openSys('settings')
    return
  }

  // 命令面板打开：键盘操作归面板自身（@keydown 在面板根），这里不抢
  if (state.paletteOpen.value) return

  if (e.key === 'Escape') {
    e.preventDefault()
    // 主屏搜索优先清查询（不清查询的 Esc 对主屏是死键）
    if (state.screen.value === 'home' && state.homeQ.value) {
      clearSearch()
    } else {
      state.goBack()
    }
    return
  }

  // 覆层打开时不再进入主屏搜索
  if (state.dialog.value || state.ctxMenu.value || state.panelOpen.value) return

  // 主屏 type-to-search：可打印字符（无修饰键）→ 聚焦隐藏输入承接（IME 合成可达）
  if (
    state.screen.value === 'home' &&
    !mod &&
    !e.altKey &&
    e.key.length === 1 &&
    !e.ctrlKey &&
    !e.metaKey
  ) {
    searchInput.value?.focus()
  }
}

// ==================== 点外部关闭覆层（面板 / 右键菜单） ====================

function onWindowClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  // 右键菜单：点菜单外关闭（菜单 Teleport 到 body，需显式判断）
  if (state.ctxMenu.value && !target.closest('.shell-ctx-root')) {
    state.ctxMenu.value = null
  }
  // 通知面板：点面板外 + 铃铛触发器外关闭
  if (
    state.panelOpen.value &&
    !target.closest('.shell-notif-panel') &&
    !target.closest('[data-shell-panel-trigger]')
  ) {
    state.panelOpen.value = false
  }
}

onMounted(() => {
  setupTheme()
  setupFontSize()
  // 壳路由挂载即拉应用清单（启动屏步骤联动；幂等）
  void appsStore.load()
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('click', onWindowClick)
})

onUnmounted(() => {
  cleanupTheme()
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('click', onWindowClick)
  if (enteringTimer) clearTimeout(enteringTimer)
})
</script>

<style scoped>
.stage {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: hidden;
}
/* 两屏常驻 DOM（v-show）：保活插件视图与滚动位置；可见性由 v-show 切换 */
.type-search-input {
  position: fixed;
  left: -9999px;
  width: 1px;
  height: 1px;
  opacity: 0;
  border: none;
  outline: none;
}
</style>
