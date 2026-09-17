<template>
  <!-- ==================== APPEARANCE ==================== -->
  <section>
    <h3 class="wb-section-title">{{ t('settings.ui.title') }}</h3>
    <div
      class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
    >
      <!-- 主题：分段控件 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
          t('settings.appearance.theme')
        }}</span>
        <div
          class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
        >
          <button
            v-for="opt in themeOptions"
            :key="opt.value"
            class="h-8 px-3 text-xs font-medium transition-colors"
            :class="
              themeValue === opt.value
                ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            "
            @click="themeValue = opt.value"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>

      <!-- 主题色板：调色台（色板卡片，切换即时生效） -->
      <div class="px-5 py-3.5 flex items-start justify-between gap-6">
        <div class="flex-shrink-0">
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.appearance.palette')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.appearance.paletteDesc') }}
          </p>
        </div>
        <div class="flex items-start gap-2 flex-wrap justify-end">
          <button
            v-for="opt in paletteOptions"
            :key="opt.value"
            class="w-[84px] rounded-[8px] border p-1.5 transition-colors"
            :class="
              paletteValue === opt.value
                ? 'border-[var(--color-primary)] bg-[var(--color-primary-light)]'
                : 'border-[var(--border-strong)] hover:border-[var(--text-tertiary)]'
            "
            :title="opt.label"
            @click="paletteValue = opt.value"
          >
            <!-- 色块预览：页面底 / 卡片底 / 强调色（取色板自身色值，预览切换后效果） -->
            <div class="flex gap-1">
              <span
                class="w-4 h-4 rounded-[3px] border border-black/5"
                :style="{ background: opt.swatches.page }"
              ></span>
              <span
                class="w-4 h-4 rounded-[3px] border border-black/5"
                :style="{ background: opt.swatches.card }"
              ></span>
              <span
                class="w-4 h-4 rounded-[3px] border border-black/5"
                :style="{ background: opt.swatches.primary }"
              ></span>
            </div>
            <p
              class="text-[calc(10px*var(--ui-scale))] text-[var(--text-secondary)] mt-1.5 text-center truncate"
            >
              {{ opt.label }}
            </p>
          </button>
        </div>
      </div>

      <!-- 语言：分段控件 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
          t('settings.appearance.language')
        }}</span>
        <div
          class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
        >
          <button
            v-for="opt in languageOptions"
            :key="opt.value"
            class="h-8 px-4 text-xs font-medium transition-colors"
            :class="
              currentLanguage === opt.value
                ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            "
            @click="onSwitchLanguage(opt.value)"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>

      <!-- 全局字体大小（终端字体在终端设置中独立配置）：小/正常/大/超大 档位间无级滑动 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
          t('settings.appearance.fontSize')
        }}</span>
        <div class="w-64 flex-shrink-0">
          <div class="flex items-center gap-3">
            <div class="flex-1">
              <input
                type="range"
                :min="MIN_FONT_SIZE"
                :max="MAX_FONT_SIZE"
                step="1"
                :value="settingsStore.settings.ui.font_size"
                class="w-full h-1 appearance-none bg-[var(--border-strong)] cursor-pointer accent-[var(--color-primary)]"
                @input="
                  settingsStore.settings.ui.font_size = Math.round(
                    Number(($event.target as HTMLInputElement).value),
                  )
                "
              />
              <!-- 档位标签：点击跳到对应档位 -->
              <div class="flex justify-between mt-1.5">
                <button
                  v-for="lvl in fontSizeLevels"
                  :key="lvl.value"
                  class="text-[calc(10px*var(--ui-scale))] transition-colors"
                  :class="
                    fontSizeLevelValue === lvl.value
                      ? 'text-[var(--text-primary)] font-medium'
                      : 'text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]'
                  "
                  @click="settingsStore.settings.ui.font_size = lvl.value"
                >
                  {{ t(lvl.key) }}
                </button>
              </div>
            </div>
            <span
              class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] w-12 text-right flex-shrink-0"
              >{{ fontSizeLevelLabel }}</span
            >
          </div>
        </div>
      </div>

      <!-- 动画效果：方角开关（关闭后全局禁用页面过渡/动画） -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.appearance.animations')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.appearance.animationsDesc') }}
          </p>
        </div>
        <button
          class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
          :class="
            animationsEnabled
              ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
              : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
          "
          role="switch"
          :aria-checked="animationsEnabled"
          @click="onToggleAnimations()"
        >
          <span
            class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
            :class="
              animationsEnabled
                ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                : 'left-[3px] bg-[var(--border-strong)]'
            "
          />
        </button>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 外观分组（SettingsView 拆分产物）
 *
 * 主题 / 色板 / 语言 / 全局字号 / 动画开关。语言切换与动画开关的状态
 * 由父组件持有（语言淡出作用于整个设置容器），此处经 props 读写。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { MIN_FONT_SIZE, MAX_FONT_SIZE, NORMAL_FONT_SIZE } from '@/composables/useFontSize'
import i18n from '@/locales'

const props = defineProps<{
  languageOptions: { value: string; label: string }[]
  currentLanguage: string
  animationsEnabled: boolean
  onSwitchLanguage: (value: string) => void
  onToggleAnimations: () => void
}>()

const { t } = useI18n()
const settingsStore = useSettingsStore()

// ==================== 字体大小档位 ====================
// 档位间可无级滑动，点击下方标签跳到对应档位；值以 px 存储（12 = 正常）
const fontSizeLevels = [
  { value: MIN_FONT_SIZE, key: 'settings.appearance.fontSmall' },
  { value: NORMAL_FONT_SIZE, key: 'settings.appearance.fontNormal' },
  { value: 14, key: 'settings.appearance.fontLarge' },
  { value: MAX_FONT_SIZE, key: 'settings.appearance.fontXl' },
]

/** 当前值最接近的档位（用于高亮标签） */
const fontSizeLevelValue = computed(() => {
  const size = settingsStore.settings.ui.font_size || NORMAL_FONT_SIZE
  return fontSizeLevels.reduce((a, b) =>
    Math.abs(b.value - size) < Math.abs(a.value - size) ? b : a,
  ).value
})

/** 当前档位文案（小 / 正常 / 大 / 超大） */
const fontSizeLevelLabel = computed(() => {
  const level = fontSizeLevels.find((l) => l.value === fontSizeLevelValue.value)
  return level ? t(level.key) : ''
})

const themeOptions = computed(() => [
  { value: 'light', label: i18n.global.t('settings.appearance.lightMode') },
  { value: 'dark', label: i18n.global.t('settings.appearance.darkMode') },
  { value: 'system', label: i18n.global.t('settings.appearance.followSystem') },
])

// 主题色板：调色台选项（色板值 + 展示色块，色块取色板自身色值以便预览切换后效果）
const paletteOptions = computed(() => [
  {
    value: 'warm',
    label: i18n.global.t('settings.appearance.paletteWarm'),
    swatches: { page: '#F5F4F0', card: '#FDFCFA', primary: '#1D1A14' },
  },
  {
    value: 'cool',
    label: i18n.global.t('settings.appearance.paletteCool'),
    swatches: { page: '#F3F5F7', card: '#FBFCFD', primary: '#2563EB' },
  },
  {
    value: 'forest',
    label: i18n.global.t('settings.appearance.paletteForest'),
    swatches: { page: '#F6F5EF', card: '#FDFCF7', primary: '#3E6B4F' },
  },
  {
    value: 'ocean',
    label: i18n.global.t('settings.appearance.paletteOcean'),
    swatches: { page: '#F2F7F9', card: '#FAFCFD', primary: '#0E7490' },
  },
  {
    value: 'sunset',
    label: i18n.global.t('settings.appearance.paletteSunset'),
    swatches: { page: '#FBF5EF', card: '#FEFAF5', primary: '#D9532A' },
  },
  {
    value: 'violet',
    label: i18n.global.t('settings.appearance.paletteViolet'),
    swatches: { page: '#F7F5FB', card: '#FCFBFE', primary: '#6D4FC6' },
  },
])

// 直接读写 store，主题切换由 useTheme 全局监听即时生效；
// setter 同时立即持久化——防抖 watch 有 500ms 窗口，切页/退出时会丢失
const themeValue = computed({
  get: () => settingsStore.settings.ui.theme,
  set: (value: string) => {
    settingsStore.settings.ui.theme = value
    void settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, theme: value },
    })
  },
})

// 色板切换由 useTheme 监听 data-palette 即时生效；同样立即持久化
// （否则切到设备页等触发 loadSettings 的页面时被后端旧值覆盖回退）
const paletteValue = computed({
  get: () => settingsStore.settings.ui.theme_palette || 'warm',
  set: (value: string) => {
    settingsStore.settings.ui.theme_palette = value
    void settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, theme_palette: value },
    })
  },
})
</script>
