<template>
  <div class="flex flex-col h-full min-h-0">
    <header class="px-4 pt-3 pb-2">
      <h1 class="text-[var(--font-size-xl)] font-semibold text-[var(--mobile-text-primary)]">
        {{ t('shell.settings.title') }}
      </h1>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== 本机 ==================== -->
      <div class="px-4 pb-1">
        <div
          class="flex items-center gap-3.5 rounded-[12px] border border-[var(--mobile-group-border)] bg-[var(--mobile-bg-card)] p-4"
          :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
        >
          <ShellBrandTile :size="48" />
          <div class="flex-1 min-w-0">
            <b class="block text-[var(--font-size-lg)] text-[var(--mobile-text-primary)] truncate">
              {{ deviceLabel }}
            </b>
            <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
              {{ t('shell.settings.runtime', { version: appVersion }) }}
            </div>
          </div>
        </div>
      </div>

      <!-- ==================== 平台 ==================== -->
      <ShellSection :title="t('shell.settings.platform')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <button
            v-for="entry in platformEntries"
            :key="entry.id"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="entry.onSelect()"
          >
            <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path :d="entry.icon" />
            </svg>
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">
              {{ entry.label }}
            </span>
            <span v-if="entry.hint" class="flex-shrink-0 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)] truncate">
              {{ entry.hint }}
            </span>
          </button>
        </ShellGroupCard>
      </div>

      <!-- ==================== 应用贡献的设置入口 ==================== -->
      <ShellSection v-if="appEntries.length > 0" :title="t('shell.settings.appSettings')" />
      <div v-if="appEntries.length > 0" class="px-4 pb-2">
        <ShellGroupCard>
          <button
            v-for="entry in appEntries"
            :key="entry.key"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="entry.entry.onSelect?.()"
          >
            <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path :d="entry.entry.icon ?? DEFAULT_ENTRY_ICON" />
            </svg>
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">
              {{ entry.entry.label }}
            </span>
            <span v-if="entry.entry.hint" class="flex-shrink-0 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)] truncate">
              {{ entry.entry.hint }}
            </span>
          </button>
        </ShellGroupCard>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 我的 / 平台设置
 *
 * 平台项是壳自带的（外观 / 权限总览 / 通知 / 关于），应用项来自各应用的
 * registerSettingsEntry——两者在同一张卡里按「平台项在前」排列，新增应用设置
 * 不需要改本文件。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { usePlatform } from '../../composables/usePlatform'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import ShellBrandTile from '../ShellBrandTile.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

/** 平台设置项（壳自带） */
interface PlatformEntry {
  id: string
  label: string
  hint?: string
  icon: string
  onSelect: () => void
}

/** 应用未自带图标时的通用图标（齿轮） */
const DEFAULT_ENTRY_ICON =
  'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z'

const { t } = useI18n()
const router = useRouter()
const nav = useShellNavigation()
const { apps } = useShellApps()
const { platformInfo } = usePlatform()

/** 版本号来自构建期注入（与设置页同源，不另立真源） */
const appVersion = __APP_VERSION__

/** 本机名：平台标识拿不到就只显示「本机」，不编造机型 */
const deviceLabel = computed(() => {
  const platform = platformInfo.value.platform
  return platform ? `${t('shell.home.localDevice')} · ${platform}` : t('shell.home.localDevice')
})

const platformEntries = computed<PlatformEntry[]>(() => [
  {
    id: 'appearance',
    label: t('shell.settings.appearance'),
    hint: t('shell.settings.appearanceHint'),
    icon: 'M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10l1.4 1.4M18.4 5.6L17 7M7 17l-1.4 1.4 M12 8a4 4 0 100 8 4 4 0 000-8z',
    // 外观沿用既有设置页：壳不与既有实现重复造一套主题配置
    onSelect: () => router.push({ name: 'mobile-settings-appearance' }),
  },
  {
    id: 'permissions',
    label: t('shell.settings.permissions'),
    hint: t('shell.settings.permissionsHint'),
    icon: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9 12l2 2 4-4',
    onSelect: () => nav.openPermissions(),
  },
  {
    id: 'notifications',
    label: t('shell.settings.notifications'),
    icon: 'M18 8a6 6 0 1 0-12 0c0 7-3 8-3 8h18s-3-1-3-8 M10.3 21a2 2 0 0 0 3.4 0',
    onSelect: () => router.push({ name: 'mobile-settings-notifications' }),
  },
  {
    id: 'about',
    label: t('shell.settings.about'),
    hint: `v${appVersion}`,
    icon: 'M12 21a9 9 0 100-18 9 9 0 000 18z M12 8h.01 M12 11v5',
    onSelect: () => router.push({ name: 'mobile-settings-about' }),
  },
])

/** 应用贡献的设置入口（按应用顺序展开） */
const appEntries = computed(() =>
  apps.value.flatMap((app) =>
    (app.contributions.settingsEntries ?? []).map((entry) => ({
      key: `${app.id}:${entry.id}`,
      entry,
    })),
  ),
)
</script>
