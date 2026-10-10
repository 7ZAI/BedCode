<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 胶囊（平台叠加在应用之上的控制项） ==================== -->
    <div class="flex justify-end px-3 pt-1.5 pb-1">
      <div
        class="flex items-center gap-0.5 rounded-[999px] border border-[var(--mobile-border)] px-1 py-0.5"
        :style="{ background: 'var(--mobile-bg-card)' }"
      >
        <button
          type="button"
          class="flex items-center justify-center rounded-[999px] min-w-[36px] min-h-[36px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
          :aria-label="t('shell.run.capsule')"
          aria-haspopup="dialog"
          @click="overlays.openCapsule(appId)"
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="5" cy="12" r="1.9" />
            <circle cx="12" cy="12" r="1.9" />
            <circle cx="19" cy="12" r="1.9" />
          </svg>
        </button>
        <span class="w-px h-4" :style="{ background: 'var(--mobile-border)' }" aria-hidden="true" />
        <button
          type="button"
          class="flex items-center justify-center rounded-[999px] min-w-[36px] min-h-[36px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
          :aria-label="t('shell.run.exitApp')"
          @click="exitApp"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <circle cx="12" cy="12" r="8" />
            <circle cx="12" cy="12" r="2.6" fill="currentColor" stroke="none" />
          </svg>
        </button>
      </div>
    </div>

    <!-- ==================== 应用运行面 ==================== -->
    <ShellAppSurface :app-id="appId" />

    <!-- ==================== Homebar：上滑并停顿呼出多任务 ==================== -->
    <button
      type="button"
      class="flex items-center justify-center w-full py-2 min-h-[32px]"
      :aria-label="t('shell.run.switcherHint')"
      @click="nav.openSwitcher()"
    >
      <span class="block w-24 h-1 rounded-full" :style="{ background: 'var(--mobile-border-active)' }" aria-hidden="true" />
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 应用运行屏
 *
 * 这一屏几乎不含平台自己的 UI：胶囊（平台叠加控制）+ homebar（呼出多任务）之外
 * 全是应用的运行面。壳不给应用加自己的标题栏/返回键，避免应用 UI 被平台框住。
 *
 * 「退出应用」只回首页，不停应用——移动端语义里退出界面 ≠ 停止后台，
 * 停止要经胶囊里的「停用此应用」或详情页操作。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useShellOverlays } from '../../composables/useShellOverlays'
import ShellAppSurface from '../ShellAppSurface.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const overlays = useShellOverlays()

const appId = computed(() => nav.params.value.appId ?? '')

function exitApp(): void {
  nav.goHome()
}
</script>
