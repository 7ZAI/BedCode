<template>
  <div class="flex flex-col h-full min-h-0">
    <header class="px-4 pt-3 pb-2">
      <h1 class="text-[var(--font-size-xl)] font-semibold text-[var(--mobile-text-primary)]">
        {{ t('shell.settings.title') }}
      </h1>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== 调试环境 ==================== -->
      <div class="px-4 pb-1">
        <div
          class="flex items-center gap-3.5 rounded-[12px] border border-[var(--mobile-group-border)] bg-[var(--mobile-bg-card)] p-4"
          :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
        >
          <ShellBrandTile :size="48" />
          <div class="flex-1 min-w-0">
            <b class="block text-[var(--font-size-lg)] text-[var(--mobile-text-primary)] truncate">
              BedCode Dev Shell
            </b>
            <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
              {{ t('shell.settings.devShellHint') }}
            </div>
          </div>
        </div>
      </div>

      <!-- ==================== 平台项 ==================== -->
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

      <!-- ==================== 外观（主题三档） ==================== -->
      <ShellSection :title="t('shell.settings.appearance')" :note="t('shell.settings.appearanceHint')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <button
            v-for="option in themeOptions"
            :key="option.value"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="setTheme(option.value)"
          >
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ option.label }}
            </span>
            <ShellChip
              v-if="theme === option.value"
              :label="t('shell.common.active')"
              tone="accent"
            />
          </button>
        </ShellGroupCard>
      </div>

      <!-- ==================== 语言 ==================== -->
      <ShellSection :title="t('shell.settings.language')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <button
            v-for="option in localeOptions"
            :key="option.value"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="setLocale(option.value)"
          >
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)]">
              {{ option.label }}
            </span>
            <ShellChip v-if="locale === option.value" :label="t('shell.common.active')" tone="accent" />
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

      <!-- ==================== 调试数据 ==================== -->
      <!-- 替代宿主的「危险操作 · 擦除本机数据」：dev-shell 的本机数据只有 localStorage
           里的调试键（插件 storage / 最近使用 / 语言），清掉即回到首次打开的状态。 -->
      <ShellSection :title="t('shell.settings.devData')" />
      <div class="px-4 pb-1">
        <button
          type="button"
          class="w-full flex items-center gap-3 px-3 py-3 text-left rounded-[12px] border transition-colors duration-200 active:opacity-80 min-h-[var(--mobile-touch-target-min)]"
          :style="{
            background: 'var(--mobile-error-muted)',
            border: '1px solid var(--mobile-error)',
            color: 'var(--mobile-error)',
          }"
          @click="clearDevData"
        >
          <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          <span class="flex-1 min-w-0 text-[var(--font-size-base)] truncate">
            {{ t('shell.settings.clearDevData') }}
          </span>
        </button>
        <p class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] px-1 mt-1.5">
          {{ t('shell.settings.clearDevDataHint') }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 我的 / 平台设置（宿主 ShellSettingsScreen.vue 的 dev-shell 同形实现）
 * -----------------------------------------------------------------------------
 * 结构与宿主一致：调试环境标识 + 平台项 + 应用贡献项。差异只在「平台项」的内容——
 * 宿主那一版是链路加密 / 生物凭证 / 出站授权 / 连接 / 认证（都是真机安全闸门，
 * 浏览器里没有），这里换成预览环境真正能改的面：权限总览、调试对象清单、清理调试数据。
 *
 * 外观 / 语言两项在宿主是「跳既有设置页」，在 dev-shell 直接就地切换（工具条里也有，
 * 两处同源：同一个 theme.ts / locale.ts）。
 *
 * 应用项来自各应用的 registerSettingsEntry——与平台项在同一张卡里按「平台项在前」排列，
 * 新增应用设置不需要改本文件。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { saveLocale, type DevLocale } from '../../../locale'
import { useDevTheme, type ThemeMode } from '../../../theme'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useToast } from '../../composables/useToast'
import ShellBrandTile from '../ShellBrandTile.vue'
import ShellChip from '../ShellChip.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

/** 应用未自带图标时的通用图标（齿轮） */
const DEFAULT_ENTRY_ICON =
  'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z'

interface PlatformEntry {
  id: string
  label: string
  hint?: string
  icon: string
  onSelect: () => void
}

const { t, locale } = useI18n()
const nav = useShellNavigation()
const toast = useToast()
const { apps, sortedApps } = useShellApps()
const { theme, setTheme } = useDevTheme()

const platformEntries = computed<PlatformEntry[]>(() => [
  {
    id: 'permissions',
    label: t('shell.settings.permissions'),
    hint: t('shell.settings.permissionsHint'),
    icon: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9 12l2 2 4-4',
    onSelect: () => nav.openPermissions(),
  },
  {
    id: 'objects',
    label: t('shell.settings.debugObjects'),
    hint: t('shell.settings.debugObjectsHint', {
      count: sortedApps.value.length,
      running: sortedApps.value.filter((a) => a.state === 'running').length,
    }),
    icon: 'M9.5 3a6.5 6.5 0 100 13 6.5 6.5 0 000-13z M14.5 12.5l6 6',
    onSelect: () => nav.switchTab('apps'),
  },
])

const themeOptions = computed(() => [
  { value: 'dark' as ThemeMode, label: t('devshell.theme.dark') },
  { value: 'light' as ThemeMode, label: t('devshell.theme.light') },
  { value: 'system' as ThemeMode, label: t('devshell.theme.system') },
])

// 语言名用自身文字展示，无需翻译
const localeOptions: { value: DevLocale; label: string }[] = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en', label: 'English' },
]

function setLocale(next: DevLocale): void {
  locale.value = next
  saveLocale(next)
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

/**
 * 清理调试数据
 *
 * 二次确认走平台对话框（不可撤销，误触代价是重新跑一遍全部 activate）。
 * 只删 dev-shell 自己写下的键（`bedcode-dev-shell:*` / `bedcode.devshell.*`），
 * 不碰同域下其他应用的数据。
 */
async function clearDevData(): Promise<void> {
  const { dialogService } = await import('../../../mock/dialog-service')
  const confirmed = await dialogService.showConfirm({
    title: t('shell.settings.clearDevData'),
    message: t('shell.settings.clearDevDataHint'),
    variant: 'warning',
    confirmText: t('shell.settings.clearDevData'),
    cancelText: t('shell.common.cancel'),
  })
  if (!confirmed) return

  const removable: string[] = []
  for (let i = 0; i < localStorage.length; i += 1) {
    const key = localStorage.key(i)
    if (key && (key.startsWith('bedcode-dev-shell:') || key.startsWith('bedcode.devshell.'))) {
      removable.push(key)
    }
  }
  for (const key of removable) localStorage.removeItem(key)
  toast.success(t('shell.settings.clearDevDataDone', { count: removable.length }))
  // 数据已清：整页重载让插件从干净状态重新 activate（不重载就会留着旧内存态）
  window.location.reload()
}
</script>