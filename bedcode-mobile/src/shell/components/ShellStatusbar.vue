<template>
  <!--
    壳内状态条
    --------------------------------------------------------------------------
    只在移动端**且系统状态栏不在屏幕上**时渲染（桌面预览没有系统状态栏，画上去反而像 bug）。
    高度 36px：状态条不参与滚动；除「点开通知栏」外不承担其它交互。

    为什么还要看安全区（2026-10-14 修正）：Android 默认显示系统状态栏，壳又画了一条自己的，
    实机就出现**上下两条时间/信号/电量**（实测截图：4:23 与 04:23 堆叠）。壳不做沉浸式全屏，
    因此判据是「顶部安全区 > 0」= 系统状态栏占位中，此时壳不再重复画一条。
  -->
  <div v-if="platformInfo.isMobile && showOwnStatusbar" class="flex-shrink-0">
    <!-- 整条可点：Android 通知栏的入口之一（另一个是主屏铃铛，44px 主入口）。
         从屏幕顶端下拉是**系统**手势，应用层不接管（见通知栏组件头注）。 -->
    <button
      type="button"
      class="flex items-center justify-between w-full px-4 h-9 transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
      :aria-label="t('shell.notify.open', { count: badgeCount })"
      @click="overlays.toggleShade()"
    >
      <span class="tabular-nums shell-type-micro" :style="{ color: 'var(--mobile-text-secondary)' }">
        {{ currentTime }}
      </span>
      <span class="flex items-center gap-1.5">
        <!-- 通知指示：有未读时显示条目数，没有则不占位（不显示「0」） -->
        <span
          v-if="badgeCount > 0"
          class="inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full text-[10px] font-semibold leading-none"
          :style="{ background: 'var(--mobile-error)', color: 'var(--mobile-text-on-accent)' }"
          aria-hidden="true"
        >
          {{ badgeCount > 99 ? '99+' : badgeCount }}
        </span>
        <!-- 信号 -->
        <svg width="17" height="12" viewBox="0 0 17 12" fill="currentColor" :style="{ color: 'var(--mobile-text-secondary)' }">
          <rect x="0" y="7.5" width="3" height="4.5" rx="1" />
          <rect x="4.6" y="5" width="3" height="7" rx="1" />
          <rect x="9.2" y="2.5" width="3" height="9.5" rx="1" />
          <rect x="13.8" y="0" width="3" height="12" rx="1" opacity=".35" />
        </svg>
        <!-- 电量 -->
        <svg width="25" height="12" viewBox="0 0 25 12" fill="none" stroke="currentColor" :style="{ color: 'var(--mobile-text-secondary)' }">
          <rect x=".6" y=".6" width="20" height="10.8" rx="3.2" stroke-width="1.2" />
          <rect x="2.6" y="2.6" width="13" height="6.8" rx="1.6" fill="currentColor" stroke="none" />
          <path d="M23 4v4" stroke-width="1.6" stroke-linecap="round" />
        </svg>
      </span>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 壳内状态条
 *
 * 平台事实的显示面 + 通知栏入口：时间 / 信号 / 电量 + 未读通知角标。
 * 角标 = 通知条数 + 待审批应用数（与通知栏头部的计数同口径）。
 */
import { computed, inject, onUnmounted, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { usePlatform } from '../composables/usePlatform'
import { useShellApps } from '../composables/useShellApps'
import { useShellNotifications } from '../composables/useShellNotifications'
import { useShellOverlays } from '../composables/useShellOverlays'

const { t } = useI18n()
const { platformInfo } = usePlatform()
const overlays = useShellOverlays()
const { notificationCount } = useShellNotifications()
const { apps } = useShellApps()

/** 顶部安全区（App.vue provide；缺省 0 = 无系统状态栏，壳自画） */
const safeArea = inject<Ref<{ top: number }>>('safeArea', ref({ top: 0 }))
const showOwnStatusbar = computed(() => (safeArea.value.top || 0) === 0)

/** 未读计数：通知 + 待审批（待审批由应用状态派生，批准后自然回落） */
const badgeCount = computed(
  () => notificationCount.value + apps.value.filter((a) => a.state === 'approval').length,
)

const currentTime = ref('')

function updateTime(): void {
  const now = new Date()
  currentTime.value = `${now.getHours().toString().padStart(2, '0')}:${now
    .getMinutes()
    .toString()
    .padStart(2, '0')}`
}

updateTime()
const timer = setInterval(updateTime, 30_000)
onUnmounted(() => clearInterval(timer))
</script>
