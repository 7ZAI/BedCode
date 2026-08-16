<script setup lang="ts">
/**
 * SettingsView — 宿主界面设置（与宿主设置页"界面设置"同构）：
 * 主题模式（浅色/深色/跟随系统）+ 主题色板（调色台）+ 语言 + 字体大小（滑杆）。
 * 全部即时生效并持久化（theme.ts），便于开发中适配宿主样式。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  MIN_FONT_SIZE,
  MAX_FONT_SIZE,
  NORMAL_FONT_SIZE,
  useHostUi,
} from '../theme'
import { saveLocale, type DevLocale } from '../locale'

const { t, locale } = useI18n()
const { theme, setTheme, palette, setPalette, fontSize, setFontSize } = useHostUi()

// ==================== 主题模式选项 ====================
const themeOptions = computed(() => [
  { value: 'light', label: t('devshell.settings.lightMode') },
  { value: 'dark', label: t('devshell.settings.darkMode') },
  { value: 'system', label: t('devshell.settings.followSystem') },
] as const)

// ==================== 主题色板：调色台选项 ====================
// 色块取色板自身色值（页面底/卡片底/强调色），预览切换后效果，与宿主取值一致
const paletteOptions = computed(() => [
  {
    value: 'warm',
    label: t('devshell.settings.paletteWarm'),
    swatches: { page: '#F5F4F0', card: '#FDFCFA', primary: '#1D1A14' },
  },
  {
    value: 'cool',
    label: t('devshell.settings.paletteCool'),
    swatches: { page: '#F3F5F7', card: '#FBFCFD', primary: '#2563EB' },
  },
  {
    value: 'forest',
    label: t('devshell.settings.paletteForest'),
    swatches: { page: '#F6F5EF', card: '#FDFCF7', primary: '#3E6B4F' },
  },
  {
    value: 'ocean',
    label: t('devshell.settings.paletteOcean'),
    swatches: { page: '#F2F7F9', card: '#FAFCFD', primary: '#0E7490' },
  },
  {
    value: 'sunset',
    label: t('devshell.settings.paletteSunset'),
    swatches: { page: '#FBF5EF', card: '#FEFAF5', primary: '#D9532A' },
  },
  {
    value: 'violet',
    label: t('devshell.settings.paletteViolet'),
    swatches: { page: '#F7F5FB', card: '#FCFBFE', primary: '#6D4FC6' },
  },
])

// ==================== 语言选项 ====================
const languageOptions = [
  { value: 'zh-CN', label: '中文' },
  { value: 'en', label: 'English' },
]

function setLocale(next: DevLocale) {
  locale.value = next
  saveLocale(next)
}

// ==================== 字体大小档位 ====================
// 档位间可无级滑动，点击下方标签跳到对应档位；值以 px 存储（12 = 正常）
const fontSizeLevels = [
  { value: MIN_FONT_SIZE, key: 'devshell.settings.fontSmall' },
  { value: NORMAL_FONT_SIZE, key: 'devshell.settings.fontNormal' },
  { value: 14, key: 'devshell.settings.fontLarge' },
  { value: MAX_FONT_SIZE, key: 'devshell.settings.fontXl' },
]

/** 当前值最接近的档位（用于高亮标签） */
const fontSizeLevelValue = computed(() =>
  fontSizeLevels.reduce((a, b) =>
    Math.abs(b.value - fontSize.value) < Math.abs(a.value - fontSize.value) ? b : a,
  ).value,
)

/** 当前档位文案（小 / 正常 / 大 / 超大） */
const fontSizeLevelLabel = computed(() => {
  const level = fontSizeLevels.find((l) => l.value === fontSizeLevelValue.value)
  return level ? t(level.key) : ''
})
</script>

<template>
  <div class="p-6">
    <h2 class="text-lg font-semibold mb-4">{{ t('devshell.nav.settings') }}</h2>

    <div class="max-w-3xl space-y-6">
      <!-- ==================== APPEARANCE（与宿主设置页同构） ==================== -->
      <section>
        <h3 class="wb-section-title">{{ t('devshell.settings.appearance') }}</h3>
        <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]">
          <!-- 主题：分段控件（浅色/深色/跟随系统） -->
          <div class="px-5 py-3.5 flex items-center justify-between gap-4">
            <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('devshell.settings.theme') }}</span>
            <div class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0">
              <button
                v-for="opt in themeOptions"
                :key="opt.value"
                class="h-8 px-3 text-xs font-medium transition-colors"
                :class="theme === opt.value
                  ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'"
                @click="setTheme(opt.value)"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>

          <!-- 主题色板：调色台（色板卡片，切换即时生效） -->
          <div class="px-5 py-3.5 flex items-start justify-between gap-6">
            <div class="flex-shrink-0">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('devshell.settings.palette') }}</span>
              <p class="text-xs text-[var(--text-tertiary)] mt-0.5">{{ t('devshell.settings.paletteDesc') }}</p>
            </div>
            <div class="flex items-start gap-2 flex-wrap justify-end">
              <button
                v-for="opt in paletteOptions"
                :key="opt.value"
                class="w-[84px] rounded-[8px] border p-1.5 transition-colors"
                :class="palette === opt.value
                  ? 'border-[var(--color-primary)] bg-[var(--color-primary-light)]'
                  : 'border-[var(--border-strong)] hover:border-[var(--text-tertiary)]'"
                :title="opt.label"
                @click="setPalette(opt.value)"
              >
                <!-- 色块预览：页面底 / 卡片底 / 强调色（取色板自身色值，预览切换后效果） -->
                <div class="flex gap-1">
                  <span class="w-4 h-4 rounded-[3px] border border-black/5" :style="{ background: opt.swatches.page }"></span>
                  <span class="w-4 h-4 rounded-[3px] border border-black/5" :style="{ background: opt.swatches.card }"></span>
                  <span class="w-4 h-4 rounded-[3px] border border-black/5" :style="{ background: opt.swatches.primary }"></span>
                </div>
                <p class="text-[calc(10px*var(--ui-scale))] text-[var(--text-secondary)] mt-1.5 text-center truncate">{{ opt.label }}</p>
              </button>
            </div>
          </div>

          <!-- 语言：分段控件 -->
          <div class="px-5 py-3.5 flex items-center justify-between gap-4">
            <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('devshell.settings.language') }}</span>
            <div class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0">
              <button
                v-for="opt in languageOptions"
                :key="opt.value"
                class="h-8 px-4 text-xs font-medium transition-colors"
                :class="locale === opt.value
                  ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'"
                @click="setLocale(opt.value)"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>

          <!-- 全局字体大小：小/正常/大/超大 档位间无级滑动 -->
          <div class="px-5 py-3.5 flex items-center justify-between gap-4">
            <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('devshell.settings.fontSize') }}</span>
            <div class="w-64 flex-shrink-0">
              <div class="flex items-center gap-3">
                <div class="flex-1">
                  <input
                    type="range"
                    :min="MIN_FONT_SIZE"
                    :max="MAX_FONT_SIZE"
                    step="0.1"
                    :value="fontSize"
                    class="w-full h-1 appearance-none bg-[var(--border-strong)] cursor-pointer accent-[var(--color-primary)]"
                    @input="setFontSize(Number(($event.target as HTMLInputElement).value))"
                  />
                  <!-- 档位标签：点击跳到对应档位 -->
                  <div class="flex justify-between mt-1.5">
                    <button
                      v-for="lvl in fontSizeLevels"
                      :key="lvl.value"
                      class="text-[calc(10px*var(--ui-scale))] transition-colors"
                      :class="fontSizeLevelValue === lvl.value
                        ? 'text-[var(--text-primary)] font-medium'
                        : 'text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]'"
                      @click="setFontSize(lvl.value)"
                    >
                      {{ t(lvl.key) }}
                    </button>
                  </div>
                </div>
                <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] w-12 text-right flex-shrink-0">{{ fontSizeLevelLabel }}</span>
              </div>
            </div>
          </div>
        </div>
      </section>

      <p class="text-xs text-[var(--text-tertiary)] leading-relaxed">{{ t('devshell.settings.note') }}</p>
    </div>
  </div>
</template>
