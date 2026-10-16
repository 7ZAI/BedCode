<template>
  <SettingsSubPage :title="t('settings.appearance.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 通用：主题、语言（分段按钮切换，替代下拉） -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ t('settings.appearance.generalSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label settings-row-label">{{ t('settings.appearance.theme') }}</span>
            <div class="settings-segment" role="group" :aria-label="t('settings.appearance.theme')">
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
          <!-- 主色色板：2026-10-16 移出宿主。主色是各 wasm-app 自持的产品外观
               （§5.1 B5 业务默认值 / 策略），选择器与真源随之下沉到应用自己的设置页
               （bedcode 见 wasm-apps/bedcode/src/settings）；本页面只留平台机制项：
               主题深浅、语言、字号缩放、主屏背景。 -->
          <div class="settings-row">
            <span class="settings-label settings-row-label">{{ t('settings.appearance.language') }}</span>
            <div class="settings-segment" role="group" :aria-label="t('settings.appearance.language')">
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
        <h2 class="settings-section-title">{{ t('settings.appearance.displaySection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ t('settings.appearance.fontSize') }}</span>
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

      <!-- 主屏背景（壁纸）：2026-10-15 由壳设置页迁入——背景属外观配置，
           与主题/色板/字体同在一个页面；档位渐变与遮罩预览复用主屏同一份
           真源（styles/wallpaper.css）。壁纸只落主屏：应用运行面保持纯色。 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ t('settings.appearance.backgroundSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ t('settings.appearance.backgroundNote') }}</span>
          </div>
          <div class="wallpaper-picker" role="group" :aria-label="t('settings.appearance.backgroundSection')">
            <button
              v-for="wall in wallpaperOptions"
              :key="wall.id"
              type="button"
              class="wallpaper-picker-btn"
              :class="{ active: wallpaper === wall.id }"
              :style="{ '--wall-dim': wallDim }"
              :aria-pressed="wallpaper === wall.id"
              @click="setWallpaper(wall.id)"
            >
              <span class="wallpaper-swatch shell-wall-swatch" :class="wall.swatchClass" aria-hidden="true" />
              <span class="min-w-0 truncate">{{ wall.label }}</span>
            </button>

            <!-- 自定义图档位：未选过时是「＋」入口，选过后是图片缩略图 -->
            <button
              type="button"
              class="wallpaper-picker-btn"
              :class="{ active: wallpaper === WALLPAPER_CUSTOM }"
              :style="{ '--wall-dim': wallDim }"
              :aria-pressed="wallpaper === WALLPAPER_CUSTOM"
              @click="onCustomWallpaperTap"
            >
              <span class="wallpaper-swatch wallpaper-swatch--custom" aria-hidden="true">
                <img v-if="wallpaperImage" :src="toFileAssetUrl(wallpaperImage)" alt="" />
                <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="3" y="5" width="18" height="14" rx="3" />
                  <path d="M3 15l4.5-4 4 3.5L15 11l6 5" />
                  <circle cx="9" cy="9.5" r="1.3" />
                </svg>
              </span>
              <span class="min-w-0 truncate">{{ t('settings.appearance.wallCustom') }}</span>
            </button>
          </div>

          <!-- 自定义图操作行：只在已经选过图时出现 -->
          <div v-if="wallpaperImage" class="flex gap-2 px-4 pb-3">
            <button
              type="button"
              class="flex-1 rounded-[10px] border border-[var(--mobile-border)] px-3 py-2 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
              :disabled="pickingWallpaper"
              @click="pickCustomWallpaper"
            >
              {{ pickingWallpaper ? '…' : t('settings.appearance.wallReplace') }}
            </button>
            <button
              type="button"
              class="flex-1 rounded-[10px] border border-[var(--mobile-border)] px-3 py-2 text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
              @click="removeCustomWallpaper"
            >
              {{ t('settings.appearance.wallRemove') }}
            </button>
          </div>

          <!-- 遮罩强度：越高越像纯色底，护壁纸上的文字对比度（滑杆值实时绑到色块预览） -->
          <div class="px-4 pb-4">
            <label class="block" :for="'appearance-wall-dim'">
              <span class="flex items-center justify-between text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
                <span>{{ t('settings.appearance.wallDim') }}</span>
                <span class="tabular-nums">{{ wallDim }}%</span>
              </span>
              <input
                id="appearance-wall-dim"
                type="range"
                min="0"
                :max="WALL_DIM_MAX"
                :value="wallDim"
                class="w-full mt-1.5"
                :style="{ accentColor: 'var(--mobile-accent)' }"
                :aria-valuetext="`${wallDim}%`"
                @input="onDimInput"
              />
            </label>
          </div>
        </div>
      </section>

      <!-- 票 2026-10-10 批次 C4：「最大可打开终端数量」是**业务项**（终端域事实），
           真源与 UI 已下沉 bedcode 应用的设置页（spec §2）。此处删除，
           避免同一上限在外观页与终端设置页各存一份、用户改了一处却不知另一处不动。 -->
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 外观设置二级页面 - 主题深浅、语言、字号缩放、主屏背景
 *
 * 本页只留**平台机制**项（§6 壳自持三类之②：外观 / 语言 / 字号缩放 / 主屏壁纸）。
 * 主色色板不在此列：主色是各 wasm-app 自持的产品外观，2026-10-16 已连同选择器与
 * 真源下沉到应用自己的设置页（bedcode 见 wasm-apps/bedcode/src/settings），
 * 宿主外壳固定用 `:root` 默认主色。
 *
 * 状态来自 useMobileSettings 共享单例（主题/语言/字号）与 useShellPreferences
 * （壁纸/遮罩），变更自动保存。
 */
import { onMounted, computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import '@/styles/wallpaper.css'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { importShellWallpaper, toFileAssetUrl } from '@/plugin/assets'
import { logger } from '@/utils/frontendLogger'
import { WALL_DIM_MAX, WALLPAPER_CUSTOM, useShellPreferences, type ShellWallpaper } from '@/shell/composables/useShellPreferences'

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

// ==================== 字体大小滑块（3 档） ====================

/** 档位顺序：normal → large → xlarge */
const FONT_STEPS = ['normal', 'large', 'xlarge'] as const
type FontStep = (typeof FONT_STEPS)[number]

/** 当前档位下标（0/1/2），非法值回退 0 */
const fontStepIndex = computed(() => {
  const idx = FONT_STEPS.indexOf(settings.value.fontSize as FontStep)
  return idx === -1 ? 0 : idx
})

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

onMounted(loadSettings)

// ==================== 主屏背景（壁纸） ====================
// 2026-10-15 由壳设置页迁入（用户指令：背景并入外观设置）；档位/遮罩与主屏
// 同一份真源（styles/wallpaper.css + useShellPreferences）。
const { wallpaper, wallpaperImage, wallDim, setWallpaper, setWallpaperImage, setWallDim } =
  useShellPreferences()

/** 壁纸档位（id + 预览类 + 本地化名），预览色块复用主屏同一份 --wall-* 渐变 */
const wallpaperOptions = computed<
  Array<{ id: ShellWallpaper; label: string; swatchClass: string }>
>(() => [
  { id: 'none', label: t('settings.appearance.wallNone'), swatchClass: 'bg-[var(--mobile-bg-elevated)]' },
  { id: 'aurora', label: t('settings.appearance.wallAurora'), swatchClass: 'shell-wall wall-aurora' },
  { id: 'dune', label: t('settings.appearance.wallDune'), swatchClass: 'shell-wall wall-dune' },
  { id: 'reef', label: t('settings.appearance.wallReef'), swatchClass: 'shell-wall wall-reef' },
])

/** 自定义壁纸选图中（按钮置灰 + 文案切加载，避免重复拉起选择器） */
const pickingWallpaper = ref(false)

/** 选图并落盘（宿主命令）：返回 null = 用户取消，不作为错误处理；选图器异常记日志，
 *  不让未处理 rejection 外泄 */
async function pickCustomWallpaper(): Promise<void> {
  if (pickingWallpaper.value) return
  pickingWallpaper.value = true
  try {
    const path = await importShellWallpaper()
    if (path !== null) setWallpaperImage(path)
  } catch (e) {
    logger.error('[Appearance] wallpaper import failed', e)
  } finally {
    pickingWallpaper.value = false
  }
}

/** 点自定义档位：已选中 → 换图；有图未选中 → 选中；无图 → 选图 */
function onCustomWallpaperTap(): void {
  if (wallpaper.value === WALLPAPER_CUSTOM) {
    void pickCustomWallpaper()
    return
  }
  if (wallpaperImage.value) {
    setWallpaper(WALLPAPER_CUSTOM)
    return
  }
  void pickCustomWallpaper()
}

/** 移除自定义图 */
function removeCustomWallpaper(): void {
  setWallpaperImage(null)
}

/** 遮罩滑杆：钳制交给偏好模块，越界值不抛错 */
function onDimInput(event: Event): void {
  const target = event.target as HTMLInputElement
  setWallDim(Number(target.value))
}
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
  font-size: clamp(0.5625rem, 0.625rem + (100vw - 360px) / 840, 0.6875rem);
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
  font-size: clamp(0.5625rem, 0.625rem + (100vw - 360px) / 840, 0.6875rem);
  color: var(--mobile-text-disabled);
}
</style>
