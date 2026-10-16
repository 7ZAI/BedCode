<template>
  <!-- 整条可点：Android 通知栏的入口之一（另一个是主屏铃铛，44px 主入口）。
       dev-shell 的舞台本身就是手机框 / 手机视口：没有系统状态栏在场，
       因此恒渲染壳自己的状态条（宿主那份会因 safeArea>0 而隐藏，dev 无此条件）。 -->
  <div class="flex-shrink-0">
    <button
      type="button"
      class="flex items-center justify-between w-full px-4 h-9 transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
      :aria-label="t('shell.notify.open', { count: badgeCount })"
      @click="overlays.toggleShade()"
    >
      <span class="tabular-nums text-[var(--font-size-xs)]" :style="{ color: 'var(--mobile-text-secondary)' }">
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
 * 壳内状态条（与宿主 ShellStatusbar.vue 同款视觉）
 *
 * 平台事实的显示面 + 通知栏入口：时间 / 信号 / 电量 + 未读通知角标。
 * 角标 = 通知条数 + 待审批应用数（与通知栏头部的计数同口径）。
 *
 * 与宿主的两处差异（都是浏览器环境使然）：
 *   · 不接 usePlatform / safeArea：宿主那份在「系统状态栏在场」时整条隐藏
 *     （避免上下两条时间/信号堆叠），dev-shell 的舞台没有系统状态栏，恒渲染；
 *   · 通知只有壳动作来源（启动 / 停止 / 审批 / 安装 / 卸载），没有宿主引擎事件。
 */
import { computed, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../composables/useShellApps'
import { useShellNotifications } from '../composables/useShellNotifications'
import { useShellOverlays } from '../composables/useShellOverlays'

const { t } = useI18n()
const overlays = useShellOverlays()
const { notificationCount } = useShellNotifications()
const { apps } = useShellApps()

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