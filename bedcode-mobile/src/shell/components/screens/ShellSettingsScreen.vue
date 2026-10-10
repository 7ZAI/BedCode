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

      <!-- ==================== 危险区：擦除本机数据 ==================== -->
      <!-- 票 2026-10-10：补回旧 SettingsView 丢失的「清除所有数据」（spec §1.6 回归项）。
           刻意留在壳而不进应用设置页——擦除对象含设备入场凭据与宿主连接态，
           按 §8 凭据零过境 / ADR 0033，插件不得触碰；见 useClearAllData.ts 头注。 -->
      <ShellSection :title="t('shell.settings.dangerZone')" />
      <div class="px-4 pb-1">
        <button
          type="button"
          class="w-full flex items-center gap-3 px-3 py-3 text-left rounded-[12px] border transition-colors duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
          :style="{
            background: 'var(--mobile-error-muted)',
            border: '1px solid var(--mobile-error)',
            color: 'var(--mobile-error)',
          }"
          @click="onClearAllData"
        >
          <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          <span class="flex-1 min-w-0 text-[var(--font-size-base)] truncate">
            {{ t('shell.settings.clearAllData') }}
          </span>
        </button>
        <p class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] px-1 mt-1.5">
          {{ t('shell.settings.clearAllDataHint') }}
        </p>
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
 * 平台项是壳自带的（链路加密 / 生物凭证 / 出站授权 / 权限总览 / 外观 / 关于），
 * 应用项来自各应用的 registerSettingsEntry——两者在同一张卡里按「平台项在前」排列，
 * 新增应用设置不需要改本文件。
 *
 * 票 2026-10-10 批次 C4：业务设置入口（连接策略 / 通知）随真源下沉 terminal-session
 * 从本表删除；壳只留平台机制与安全闸门，避免同一个开关在壳与应用各存一份真值。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { clearAllData } from '@/composables/useClearAllData'
import { confirm } from '@tauri-apps/plugin-dialog'
import { logger } from '@/utils/frontendLogger'
import { usePlatform } from '../../composables/usePlatform'
import { useToast } from '../../composables/useToast'
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
const toast = useToast()
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
    id: 'link-encryption',
    label: t('shell.settings.connection'),
    hint: t('shell.settings.connectionHint'),
    icon: 'M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71 M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71',
    // 票 2026-10-10 批次 C4：本入口只剩**平台项**（链路加密，ADR 0022 ②类安全闸门）。
    // 原先同页的自动重连 / 保持连接 / 默认端口是业务设置，已下沉 terminal-session。
    onSelect: () => router.push({ name: 'mobile-settings-connection' }),
  },
  {
    // 票 2026-10-10 C4：只剩生物凭证（安全面）。「首选认证方式」是业务设置，已下沉。
    id: 'biometric',
    label: t('shell.settings.authentication'),
    hint: t('shell.settings.authenticationHint'),
    icon: 'M7 11V8a5 5 0 0110 0v3 M5 11h14v10H5z',
    onSelect: () => router.push({ name: 'mobile-settings-authentication' }),
  },
  {
    id: 'egress',
    label: t('shell.settings.egress'),
    hint: t('shell.settings.egressHint'),
    icon: 'M14 5h6v6 M20 5l-8 8 M18 14v4a2 2 0 01-2 2H6a2 2 0 01-2-2V8a2 2 0 012-2h4',
    onSelect: () => router.push({ name: 'mobile-settings-egress' }),
  },
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
    id: 'about',
    label: t('shell.settings.about'),
    hint: `v${appVersion}`,
    icon: 'M12 21a9 9 0 100-18 9 9 0 000 18z M12 8h.01 M12 11v5',
    onSelect: () => router.push({ name: 'mobile-settings-about' }),
  },
])

/**
 * 擦除本机数据
 *
 * 二次确认用宿主 confirm 对话框：这是不可撤销的动作，误触代价是重新配对 + 重建全部配置。
 * 确认后执行；断连失败不阻断清理（用户的目标是清数据，不是断连），
 * 但清理失败必须如实告知——不能 reload 完就说成功。
 */
async function onClearAllData(): Promise<void> {
  const confirmed = await confirm(
    t('shell.settings.clearAllData'),
    { title: t('shell.settings.clearAllData'), kind: 'warning' },
  )
  if (!confirmed) return

  const result = await clearAllData()
  if (!result.completed) {
    // 清理失败时不能 reload——那会让用户以为已经清干净
    logger.error(`[ShellSettings] clear all data incomplete: ${result.error ?? 'unknown'}`)
    toast.error(t('shell.settings.clearAllDataFailed'))
  }
}

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
