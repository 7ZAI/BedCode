<template>
  <SettingsSubPage :title="$t('settings.appearance.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 通用：主题、语言（分段按钮切换，替代下拉） -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.generalSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label settings-row-label">{{ $t('settings.appearance.theme') }}</span>
            <div class="settings-segment" role="group" :aria-label="$t('settings.appearance.theme')">
              <button
                v-for="opt in themeOptions"
                :key="opt.value"
                type="button"
                class="settings-segment-btn"
                :class="{ active: themeMode === opt.value }"
                @click="themeMode = opt.value"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>
          <div class="settings-row">
            <span class="settings-label settings-row-label">{{ $t('settings.appearance.language') }}</span>
            <div class="settings-segment" role="group" :aria-label="$t('settings.appearance.language')">
              <button
                v-for="opt in languageOptions"
                :key="opt.value"
                type="button"
                class="settings-segment-btn"
                :class="{ active: currentLanguage === opt.value }"
                @click="currentLanguage = opt.value"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- 显示：字体大小（3 档滑块） -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.appearance.displaySection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.appearance.fontSize') }}</span>
            <span class="settings-value">{{ currentFontSizeLabel }}</span>
          </div>
          <div class="px-4 pb-4">
            <div
              ref="fontSliderTrackRef"
              class="font-size-slider"
              @pointerdown="onFontSliderPointerDown"
            >
              <div class="font-size-slider-fill" :style="fontSliderFillStyle"></div>
              <div class="font-size-slider-dots">
                <span
                  v-for="i in FONT_STEPS.length"
                  :key="i"
                  class="font-size-slider-dot"
                  :class="{ active: i - 1 <= fontStepIndex }"
                ></span>
              </div>
              <div class="font-size-slider-thumb" :style="fontSliderThumbStyle"></div>
            </div>
            <div class="font-size-slider-labels">
              <span>{{ t('settings.appearance.fontNormal') }}</span>
              <span>{{ t('settings.appearance.fontLarge') }}</span>
              <span>{{ t('settings.appearance.fontXLarge') }}</span>
            </div>
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
import { onMounted, computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
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

/** 字体大小选项（i18n 标签，值保持与 store 一致） */
const fontSizeOptions = computed(() => [
  { value: 'normal', label: t('settings.appearance.fontNormal') },
  { value: 'large', label: t('settings.appearance.fontLarge') },
  { value: 'xlarge', label: t('settings.appearance.fontXLarge') },
])

// ==================== 字体大小滑块（3 档） ====================

/** 档位顺序：normal → large → xlarge */
const FONT_STEPS = ['normal', 'large', 'xlarge'] as const
type FontStep = (typeof FONT_STEPS)[number]

/** 当前档位下标（0/1/2），非法值回退 0 */
const fontStepIndex = computed(() => {
  const idx = FONT_STEPS.indexOf(settings.value.fontSize as FontStep)
  return idx === -1 ? 0 : idx
})

/** 当前档位名称（滑块右侧同步显示） */
const currentFontSizeLabel = computed(() =>
  fontSizeOptions.value.find(o => o.value === settings.value.fontSize)?.label ?? ''
)

const fontSliderTrackRef = ref<HTMLElement | null>(null)

/** 滑块填充宽度（按档位百分比） */
const fontSliderFillStyle = computed(() => ({
  width: `${(fontStepIndex.value / (FONT_STEPS.length - 1)) * 100}%`,
}))

/** 滑块拇指位置 */
const fontSliderThumbStyle = computed(() => ({
  left: `${(fontStepIndex.value / (FONT_STEPS.length - 1)) * 100}%`,
}))

/** 根据指针在轨道上的位置吸附到最近档位 */
function setFontStepFromPointer(clientX: number) {
  const track = fontSliderTrackRef.value
  if (!track) return
  const rect = track.getBoundingClientRect()
  const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width))
  const idx = Math.round(ratio * (FONT_STEPS.length - 1))
  settings.value.fontSize = FONT_STEPS[idx]
}

function onFontSliderPointerDown(e: PointerEvent) {
  setFontStepFromPointer(e.clientX)
  const track = fontSliderTrackRef.value
  if (track) track.setPointerCapture(e.pointerId)

  const onMove = (ev: PointerEvent) => setFontStepFromPointer(ev.clientX)
  const onUp = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
}

/** 将最大可打开终端数量限制在 1-20，非法输入回退默认值 */
function clampMaxOpenTerminals() {
  const v = Number(settings.value.maxOpenTerminals)
  settings.value.maxOpenTerminals = Number.isFinite(v) && v > 0
    ? Math.min(20, Math.round(v))
    : defaultMobileSettings.maxOpenTerminals
}

onMounted(loadSettings)
</script>

<style scoped>
/* ==================== 分段按钮组（主题/语言切换，替代下拉） ==================== */

/* 设置项标签：固定单行，不允许换行 */
.settings-row-label {
  white-space: nowrap;
  flex-shrink: 0;
}

.settings-segment {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  align-items: center;
  gap: 0.25rem;
  padding: 0.25rem;
  border-radius: 0.75rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

.settings-segment-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 1.875rem;
  padding: 0.25rem 0.5rem;
  border-radius: 0.5rem;
  border: none;
  background: transparent;
  color: var(--mobile-text-secondary);
  font-size: clamp(0.5625rem, 0.625rem + (100vw - 360px) / 840 * 0.0625rem, 0.6875rem);
  white-space: nowrap;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-segment-btn:active {
  opacity: 0.8;
}

.settings-segment-btn.active {
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
  font-weight: 500;
  box-shadow: 0 1px 4px color-mix(in srgb, var(--mobile-accent) 40%, transparent);
}

/* ==================== 字体大小滑块（3 档，自绘外观） ==================== */

.font-size-slider {
  position: relative;
  height: 28px;
  cursor: pointer;
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
}

.font-size-slider::before {
  content: '';
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  height: 4px;
  transform: translateY(-50%);
  background: var(--mobile-bg-elevated);
  border-radius: 2px;
}

.font-size-slider-fill {
  position: absolute;
  top: 50%;
  left: 0;
  height: 4px;
  transform: translateY(-50%);
  background: var(--mobile-accent);
  border-radius: 2px;
  pointer-events: none;
}

.font-size-slider-thumb {
  position: absolute;
  top: 50%;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--mobile-accent);
  transform: translate(-50%, -50%);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--mobile-accent) 20%, transparent);
  transition: box-shadow 0.15s ease;
  z-index: 2;
}

.font-size-slider-thumb:hover {
  box-shadow: 0 0 0 5px color-mix(in srgb, var(--mobile-accent) 30%, transparent);
}

.font-size-slider-dots {
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  transform: translateY(-50%);
  display: flex;
  justify-content: space-between;
  padding: 0 1px;
  pointer-events: none;
  z-index: 1;
}

.font-size-slider-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--mobile-border);
  transition: background 0.15s ease;
}

.font-size-slider-dot.active {
  background: var(--mobile-accent);
}

.font-size-slider-labels {
  display: flex;
  justify-content: space-between;
  margin-top: 0.25rem;
  font-size: clamp(0.5625rem, 0.625rem + (100vw - 360px) / 840 * 0.0625rem, 0.6875rem);
  color: var(--mobile-text-disabled);
}
</style>
