<template>
  <SettingsSubPage :title="$t('settings.appearance.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 通用：主题、语言 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.generalSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.theme') }}</span>
            <Select v-model="themeMode" :options="themeOptions" class="min-w-[7.5rem]" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.language') }}</span>
            <Select v-model="currentLanguage" :options="languageOptions" class="min-w-[7.5rem]" />
          </div>
        </div>
      </section>

      <!-- 显示：字体大小 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.displaySection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.fontSize') }}</span>
            <Select v-model="settings.fontSize" :options="fontSizeOptions" class="min-w-[7.5rem]" />
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
import { onMounted, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import Select from '@bedcode/plugin-sdk-mobile/ui'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { useMobileSettings, defaultMobileSettings } from '@/composables/useMobileSettings'

const { t } = useI18n()

const { settings, themeMode, currentLanguage, loadSettings } = useMobileSettings()

/** 主题选项（i18n 标签，值保持与 store 一致） */
const themeOptions = computed(() => [
  { value: 'dark', label: t('settings.appearance.darkMode') },
  { value: 'light', label: t('settings.appearance.lightMode') },
  { value: 'system', label: t('settings.appearance.followSystem') },
])

/** 语言选项（语言名用各自原生写法，两种语言环境保持一致） */
const languageOptions = computed(() => [
  { value: 'zh-CN', label: t('settings.appearance.languageChinese') },
  { value: 'en', label: t('settings.appearance.languageEnglish') },
])

/** 字体大小选项 */
const fontSizeOptions = computed(() => [
  { value: 'normal', label: t('settings.appearance.fontNormal') },
  { value: 'large', label: t('settings.appearance.fontLarge') },
  { value: 'xlarge', label: t('settings.appearance.fontXLarge') },
])

/** 将最大可打开终端数量限制在 1-20，非法输入回退默认值 */
function clampMaxOpenTerminals() {
  const v = Number(settings.value.maxOpenTerminals)
  settings.value.maxOpenTerminals = Number.isFinite(v) && v > 0
    ? Math.min(20, Math.round(v))
    : defaultMobileSettings.maxOpenTerminals
}

onMounted(loadSettings)
</script>
