<template>
  <div class="flex flex-col h-full min-h-0">
    <div class="flex items-center justify-between gap-2 px-4 pt-3 pb-2">
      <span class="text-[var(--font-size-lg)] font-semibold text-[var(--mobile-text-primary)]">
        {{ t('shell.switcher.title') }}
      </span>
      <ShellChip :label="t('shell.switcher.backHome')" tone="accent" clickable @click="nav.goHome()" />
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto px-4 pb-4">
      <div v-if="runningApps.length > 0" class="flex flex-col gap-3">
        <div
          v-for="app in runningApps"
          :key="app.id"
          class="overflow-hidden rounded-[12px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)]"
          :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
        >
          <button
            type="button"
            class="flex items-center gap-2.5 w-full px-3 py-2.5 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="enter(app.id)"
          >
            <ShellAppIcon :app="app" :size="30" />
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)] truncate">
              {{ app.name }}
            </span>
            <ShellChip :label="t('shell.common.running')" tone="success" />
          </button>

          <!-- 预览区：平台不持有应用内部状态，这里只给出入口与停止操作，
               不编造「最近输出」之类的假内容 -->
          <div class="flex items-center justify-between gap-2 px-3 pb-3">
            <span class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
              {{ t('shell.apps.permissionCount', { count: app.permissions.length }) }}
            </span>
            <ShellChip :label="t('shell.switcher.stop')" tone="danger" clickable @click="stopApp(app.id)" />
          </div>
        </div>
      </div>

      <p
        v-else
        class="mt-6 rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-6 text-center text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)]"
      >
        {{ t('shell.switcher.empty') }}
      </p>

      <p class="mt-4 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
        {{ t('shell.switcher.foot') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 多任务（运行中的应用）
 *
 * 平台只列出「谁在跑」并给进入/停止两个动作。应用内部快照属于应用自己，
 * 平台没有也不该去抓——没有真源就把预览区留白并说明，而不是生成假内容。
 */
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellChip from '../ShellChip.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const { runningApps, stop } = useShellApps()

/** 进入已运行的应用运行面 */
function enter(appId: string): void {
  nav.navigate('app-run', { appId })
}

async function stopApp(appId: string): Promise<void> {
  await stop(appId)
}
</script>
