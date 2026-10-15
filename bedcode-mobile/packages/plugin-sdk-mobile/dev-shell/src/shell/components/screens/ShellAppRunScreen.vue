<template>
  <div class="flex flex-col h-full min-h-0">
    <!--
      平台顶栏：左侧应用身份（图标 + 名称），右侧胶囊（菜单 / 多任务 / 回主屏）。
      三个按钮都是显式点击路径，不依赖系统手势；胶囊对齐小程序形态——平台的
      控制项浮在应用之上，但**不**在界面上讲解自己是什么（2026-10-14 修订）。
    -->
    <header class="flex items-center gap-2.5 px-3 pt-1.5 pb-1 flex-shrink-0">
      <ShellAppIcon v-if="app" :app="app" :size="30" />
      <span
        class="flex-1 min-w-0 text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate"
      >
        {{ app?.name ?? t('shell.run.notFound') }}
      </span>

      <div class="shell-capsule">
        <button
          type="button"
          class="shell-capsule-btn"
          :aria-label="t('shell.run.menu')"
          aria-haspopup="dialog"
          @click="overlays.openCapsule(appId)"
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <circle cx="5" cy="12" r="1.9" />
            <circle cx="12" cy="12" r="1.9" />
            <circle cx="19" cy="12" r="1.9" />
          </svg>
        </button>
        <span class="shell-capsule-divider" aria-hidden="true" />
        <button
          type="button"
          class="shell-capsule-btn"
          :aria-label="t('shell.run.recents')"
          @click="nav.openSwitcher()"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="7" y="3" width="14" height="14" rx="3" />
            <path d="M3 7v10a4 4 0 0 0 4 4h10" />
          </svg>
        </button>
        <span class="shell-capsule-divider" aria-hidden="true" />
        <button
          type="button"
          class="shell-capsule-btn"
          :aria-label="t('shell.run.home')"
          @click="nav.goHome()"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M3 10.5L12 3l9 7.5" />
            <path d="M5 9.5V21h14V9.5" />
          </svg>
        </button>
      </div>
    </header>

    <!-- ==================== 应用运行面 ==================== -->
    <ShellAppSurface :app-id="appId" />
  </div>
</template>

<script setup lang="ts">
/**
 * 应用运行屏
 *
 * 平台只叠加两样东西：顶部一行应用身份，与右侧胶囊（菜单 / 多任务 / 回主屏）。
 * 应用内部界面由应用自持（ShellAppSurface），平台不画应用内页头，也不给应用
 * 加返回键——返回不是这一层的语义，退出走「回主屏」。
 *
 * 底部系统条已在 2026-10-14 修订中整条退役：入口收进本屏胶囊（多任务 / 回主屏），
 * 非根屏的返回由各屏自己的页头承担。这样平台导航条不再长期占用应用的可视高度。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useShellOverlays } from '../../composables/useShellOverlays'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellAppSurface from '../ShellAppSurface.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const overlays = useShellOverlays()
const { getApp } = useShellApps()

const appId = computed(() => nav.params.value.appId ?? '')
const app = computed(() => getApp(appId.value))
</script>
