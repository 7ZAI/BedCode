<template>
  <SettingsSubPage :title="$t('settings.appearance.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 通用：主题、语言 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.generalSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.theme') }}</span>
            <select v-model="themeMode" class="settings-select">
              <option value="dark">{{ $t('settings.appearance.darkMode') }}</option>
              <option value="light">{{ $t('settings.appearance.lightMode') }}</option>
              <option value="system">{{ $t('settings.appearance.followSystem') }}</option>
            </select>
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.language') }}</span>
            <select v-model="currentLanguage" class="settings-select">
              <option value="zh-CN">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>
      </section>

      <!-- 显示：字体大小 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.displaySection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.fontSize') }}</span>
            <select v-model="settings.fontSize" class="settings-select">
              <option value="normal">{{ $t('settings.appearance.fontNormal') }}</option>
              <option value="large">{{ $t('settings.appearance.fontLarge') }}</option>
              <option value="xlarge">{{ $t('settings.appearance.fontXLarge') }}</option>
            </select>
          </div>
        </div>
      </section>

      <!-- 终端：最大打开数量 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.terminalSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <div class="min-w-0">
              <div class="settings-label">{{ $t('settings.appearance.maxOpenTerminals') }}</div>
              <div class="settings-desc">{{ $t('settings.appearance.maxOpenTerminalsDesc') }}</div>
            </div>
            <input
              v-model.number="settings.maxOpenTerminals"
              type="number"
              inputmode="numeric"
              min="1"
              max="20"
              class="settings-number-input shrink-0"
              @change="clampMaxOpenTerminals"
              @blur="clampMaxOpenTerminals"
            />
          </div>
        </div>
      </section>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 外观设置二级页面 - 主题、语言、字体大小、最大可打开终端数量
 * 状态来自 useMobileSettings 共享单例，变更自动保存
 */
import { onMounted } from 'vue'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { useMobileSettings, defaultMobileSettings } from '@/composables/useMobileSettings'

const { settings, themeMode, currentLanguage, loadSettings } = useMobileSettings()

/** 将最大可打开终端数量限制在 1-20，非法输入回退默认值 */
function clampMaxOpenTerminals() {
  const v = Number(settings.value.maxOpenTerminals)
  settings.value.maxOpenTerminals = Number.isFinite(v) && v > 0
    ? Math.min(20, Math.round(v))
    : defaultMobileSettings.maxOpenTerminals
}

onMounted(loadSettings)
</script>
