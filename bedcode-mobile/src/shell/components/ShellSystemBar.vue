<template>
  <!--
    底部系统条（Android 导航条语义：返回 / 主屏 / 多任务）
    --------------------------------------------------------------------------
    壳内唯一的常驻导航面。旧底部页签（首页/应用/我的）已退役：Android 启动器
    没有应用级页签，平台入口收进主屏的「系统」宫格，导航收进本条。
    三个按钮都是 44px 触控目标；中间的主屏键额外接受「上滑呼出多任务」手势——
    但手势**不是唯一路径**（WCAG 2.2 Dragging Movements）：右侧多任务键与之等效。
  -->
  <nav
    class="shell-system-bar flex items-stretch flex-shrink-0"
    :style="{ paddingBottom: `${bottomInset}px` }"
    :aria-label="t('shell.systemBar.label')"
  >
    <!-- 返回：已在栈底时禁用（不隐藏——位置固定才不会让另两个键左右跳动） -->
    <button
      type="button"
      class="flex-1 flex items-center justify-center min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] disabled:opacity-35"
      :disabled="!nav.canBack.value"
      :aria-label="t('shell.systemBar.back')"
      @click="nav.back()"
    >
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M15 6l-6 6 6 6" />
      </svg>
    </button>

    <!-- 主屏：点击回主屏，上滑呼出多任务 -->
    <button
      type="button"
      class="flex-1 flex items-center justify-center min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
      style="touch-action: none"
      :aria-label="t('shell.systemBar.home')"
      @click="goHome"
      @pointerdown="onPointerDown"
      @pointerup="onPointerUp"
      @pointercancel="resetPointer"
    >
      <span
        class="block w-16 h-1 rounded-full"
        :style="{ background: 'var(--mobile-border-active)' }"
        aria-hidden="true"
      />
    </button>

    <!-- 多任务：卡片式切换入口（与上滑手势等效的显式路径） -->
    <button
      type="button"
      class="flex-1 flex items-center justify-center min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
      :aria-label="t('shell.systemBar.recents')"
      @click="nav.openSwitcher()"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <rect x="7" y="3" width="14" height="14" rx="3" />
        <path d="M3 7v10a4 4 0 0 0 4 4h10" />
      </svg>
    </button>
  </nav>
</template>

<script setup lang="ts">
/**
 * 底部系统条
 *
 * 手势判定只在主屏键自身：位移超过阈值判「上滑呼多任务」，否则交给 click 走「回主屏」。
 * 不抢页面滚动（系统条不在滚动区内），也不接管屏幕边缘手势（那是 Android 系统手势，
 * 应用层覆盖会与之冲突——ui-ux-pro-max `--domain ux` Gesture Conflicts）。
 */
import { computed, inject, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellNavigation } from '../composables/useShellNavigation'

const { t } = useI18n()
const nav = useShellNavigation()

// 安全区由 App.vue 注入（Android WebView 不支持 env()）；缺省 0 = 无导航栏占位
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar?: number }> | undefined>(
  'safeArea',
  undefined,
)
const bottomInset = computed(() => {
  const value = safeArea?.value
  return value?.navigationBar ?? value?.bottom ?? 0
})

/** 上滑判定阈值：小于该位移视为点击（避免手抖把点击判成手势） */
const SWIPE_UP_THRESHOLD = 28
const pointerStartY = ref<number | null>(null)

function onPointerDown(event: PointerEvent): void {
  pointerStartY.value = event.clientY
}

function onPointerUp(event: PointerEvent): void {
  const start = pointerStartY.value
  pointerStartY.value = null
  if (start === null) return
  // 上滑 = clientY 变小；达到阈值则呼出多任务，并吞掉这次 click
  if (start - event.clientY >= SWIPE_UP_THRESHOLD) {
    event.preventDefault()
    nav.openSwitcher()
  }
}

function resetPointer(): void {
  pointerStartY.value = null
}

function goHome(): void {
  nav.goHome()
}
</script>
