<template>
  <SettingsSubPage :title="$t('settings.appearance.title')">
    <div class="px-4 py-4 space-y-4">
      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.theme') }}</span>
        <select
          v-model="themeMode"
          class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1.5 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
        >
          <option value="dark">{{ $t('settings.appearance.darkMode') }}</option>
          <option value="light">{{ $t('settings.appearance.lightMode') }}</option>
          <option value="system">{{ $t('settings.appearance.followSystem') }}</option>
        </select>
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.language') }}</span>
        <select
          v-model="currentLanguage"
          class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1.5 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
        >
          <option value="zh-CN">中文</option>
          <option value="en">English</option>
        </select>
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.fontSize') }}</span>
        <select
          v-model="settings.fontSize"
          class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
        >
          <option value="small">{{ $t('settings.appearance.fontSmall') }}</option>
          <option value="medium">{{ $t('settings.appearance.fontMedium') }}</option>
          <option value="large">{{ $t('settings.appearance.fontLarge') }}</option>
        </select>
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.terminalCacheCount') }}</span>
        <input
          v-model.number="settings.maxCachedTerminals"
          type="number"
          min="1"
          max="50"
          class="w-16 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
        />
      </div>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 外观设置二级页面 - 主题、语言、字体大小、终端缓存数量
 * 状态来自 useMobileSettings 共享单例，变更自动保存
 */
import { onMounted } from 'vue'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'

const { settings, themeMode, currentLanguage, loadSettings } = useMobileSettings()

onMounted(loadSettings)
</script>
