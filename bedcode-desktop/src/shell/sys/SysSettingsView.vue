<template>
  <!-- ==================== 界面（壳内实现：主题/色板/语言/字号/动画/导航按钮） ==================== -->
  <ShellCard :title="t('shell.settings.ui.title')" icon="theme">
    <!-- 主题：三档分段控件 -->
    <div class="row-sw">
      <div class="lb">
        <b>{{ t('settings.appearance.theme') }}</b>
        <span>{{ t('shell.settings.ui.themeDesc') }}</span>
      </div>
      <ShellSeg
        :options="themeOptions"
        :model-value="settingsStore.settings.ui.theme"
        @update:model-value="setTheme"
      />
    </div>

    <!-- 色板 -->
    <div class="fld">
      <span class="lb">{{ t('settings.appearance.palette') }}</span>
      <div class="pal-grid">
        <button
          v-for="p in palettes"
          :key="p.id"
          class="pal-card"
          :class="{ on: settingsStore.settings.ui.theme_palette === p.id }"
          @click="setPalette(p.id)"
        >
          <span class="pal-sw">
            <i :style="{ background: p.sw[0] }"></i>
            <i :style="{ background: p.sw[1] }"></i>
            <i :style="{ background: p.sw[2] }"></i>
          </span>
          <span class="nm">{{ p.name }}</span>
        </button>
      </div>
    </div>

    <!-- 语言 -->
    <div class="row-sw">
      <div class="lb">
        <b>{{ t('shell.settings.ui.language') }}</b>
        <span>{{ t('shell.settings.ui.languageDesc') }}</span>
      </div>
      <select
        class="shell-inp"
        style="width: 130px"
        :value="i18nLocale"
        @change="setLanguage($event)"
      >
        <option value="zh-CN">简体中文</option>
        <option value="en">English</option>
      </select>
    </div>

    <!-- 字号 -->
    <div class="fld">
      <span class="lb"
        >{{ t('shell.settings.ui.fontSize') }} ·
        <span class="mono">{{ settingsStore.settings.ui.font_size }}px</span></span
      >
      <div class="size-row">
        <input
          type="range"
          min="10"
          max="16"
          step="2"
          :value="settingsStore.settings.ui.font_size"
          :aria-label="t('shell.settings.ui.fontSize')"
          @input="setFontSize(Number(($event.target as HTMLInputElement).value))"
        />
        <span class="val">{{ fontSizeLabel }}</span>
      </div>
    </div>

    <!-- 动画 -->
    <div class="row-sw">
      <div class="lb">
        <b>{{ t('shell.settings.ui.animations') }}</b>
        <span>{{ t('shell.settings.ui.animationsDesc') }}</span>
      </div>
      <label class="shell-sw">
        <input
          type="checkbox"
          :checked="settingsStore.settings.ui.animations_enabled !== false"
          :aria-label="t('shell.settings.ui.animations')"
          @change="toggleAnimations"
        />
        <span class="track"></span><span class="knob"></span>
      </label>
    </div>

    <!-- 导航按钮显隐（spec-c §6：隐藏后 Esc 仍全程等效返回） -->
    <div class="row-sw">
      <div class="lb">
        <b>{{ t('shell.settings.ui.showNav') }}</b>
        <span>{{ t('shell.settings.ui.showNavDesc') }}</span>
      </div>
      <label class="shell-sw">
        <input
          type="checkbox"
          :checked="prefs.showNav"
          :aria-label="t('shell.settings.ui.showNav')"
          @change="prefs.showNav = !prefs.showNav"
        />
        <span class="track"></span><span class="knob"></span>
      </label>
    </div>
  </ShellCard>

  <!-- ==================== 背景（壳专属：token 派生渐变壁纸 + 遮罩强度） ==================== -->
  <ShellCard :title="t('shell.settings.background.title')" icon="layers">
    <div class="fld">
      <span class="lb">{{ t('shell.settings.background.wallLabel') }}</span>
      <span class="hint">{{ t('shell.settings.background.wallHint') }}</span>
      <div class="pal-grid">
        <button
          v-for="w in wallpapers"
          :key="w.id"
          class="pal-card"
          :class="{ on: prefs.wallpaper === w.id }"
          @click="prefs.wallpaper = w.id"
        >
          <span class="pal-sw" :class="w.id === 'none' ? '' : ['wall-swatch', 'wall-' + w.id]">
            <i v-if="w.id === 'none'" :style="{ background: 'var(--bg-page)' }"></i>
          </span>
          <span class="nm">{{ t(w.nameKey) }}</span>
        </button>
      </div>
    </div>
    <div class="fld">
      <span class="lb">{{ t('shell.settings.background.dimLabel', { pct: prefs.wallDim }) }}</span>
      <span class="hint">{{ t('shell.settings.background.dimHint') }}</span>
      <div class="size-row">
        <input
          type="range"
          min="0"
          max="70"
          step="10"
          :value="prefs.wallDim"
          :aria-label="t('shell.settings.background.dimLabel', { pct: prefs.wallDim })"
          @input="prefs.wallDim = Number(($event.target as HTMLInputElement).value)"
        />
        <span class="val">{{ prefs.wallDim }}%</span>
      </div>
    </div>
    <div class="fld">
      <span class="hint">{{ t('shell.settings.background.customHint') }}</span>
      <div>
        <button class="shell-mini" disabled :title="t('shell.settings.background.customPending')">
          <ShellIcon name="plus" :size="12" /> {{ t('shell.settings.background.custom') }}
        </button>
      </div>
    </div>
  </ShellCard>

  <!-- ==================== 链路加密 / 系统 / 日志 / 关于（复用既有设置分组组件，零重写） ==================== -->
  <SettingsLinkCryptoSection />
  <SettingsSystemSection />
  <SettingsLoggingSection />
  <SettingsAboutSection />

  <!-- 授权管理入口（一级系统页互切，spec-c §3） -->
  <ShellCard :title="t('shell.settings.permsEntry.title')" icon="shield">
    <div class="row-sw">
      <div class="lb">
        <b>{{ t('shell.settings.permsEntry.desc') }}</b>
        <span>{{ t('shell.settings.permsEntry.hint') }}</span>
      </div>
      <button class="shell-mini" @click="state.openSys('perms')">
        <ShellIcon name="shield" :size="12" /> {{ t('shell.settings.permsEntry.open') }}
      </button>
    </div>
  </ShellCard>
</template>

<script setup lang="ts">
/**
 * SysSettingsView — 系统页「设置」（壳内实现）
 *
 * 界面 / 背景两组是壳专属（含导航按钮显隐、壁纸，纯壳偏好落 useShellSettings）；
 * 链路加密 / 系统 / 日志 / 关于直接复用既有设置分组组件（真实命令面，零重写）。
 * 授权管理入口互切到「权限与授权」系统页卡（spec-c §3 同构卡片）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useFontSize } from '@/composables/useFontSize'
import SettingsLinkCryptoSection from '@/components/settings/SettingsLinkCryptoSection.vue'
import SettingsSystemSection from '@/components/settings/SettingsSystemSection.vue'
import SettingsLoggingSection from '@/components/settings/SettingsLoggingSection.vue'
import SettingsAboutSection from '@/components/settings/SettingsAboutSection.vue'
import { useShellState } from '../useShellState'
import { useShellSettings } from '../useShellSettings'
import { WALLPAPERS } from '../state'
import ShellIcon from '../components/ShellIcon.vue'
import ShellCard from '../components/ShellCard.vue'
import ShellSeg from '../components/ShellSeg.vue'

const { t, locale } = useI18n()
const i18nLocale = computed(() => locale.value)
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const { setupFontSize } = useFontSize()
const state = useShellState()
const { prefs } = useShellSettings()

const themeOptions = computed(() => [
  { value: 'light', label: t('shell.settings.ui.themeLight') },
  { value: 'dark', label: t('shell.settings.ui.themeDark') },
  { value: 'system', label: t('shell.settings.ui.themeSystem') },
])

const palettes = computed(() => [
  { id: 'warm', name: t('settings.appearance.paletteWarm'), sw: ['#f5f4f0', '#fdfcfa', '#1d1a14'] },
  { id: 'cool', name: t('settings.appearance.paletteCool'), sw: ['#f3f5f7', '#fbfcfd', '#2563eb'] },
  {
    id: 'forest',
    name: t('settings.appearance.paletteForest'),
    sw: ['#f6f5ef', '#fdfcf7', '#3e6b4f'],
  },
  {
    id: 'ocean',
    name: t('settings.appearance.paletteOcean'),
    sw: ['#f2f7f9', '#fafcfd', '#0e7490'],
  },
  {
    id: 'sunset',
    name: t('settings.appearance.paletteSunset'),
    sw: ['#fbf5ef', '#fefaf5', '#d9532a'],
  },
  {
    id: 'violet',
    name: t('settings.appearance.paletteViolet'),
    sw: ['#f7f5fb', '#fcfbfe', '#6d4fc6'],
  },
])

const wallpapers = computed(() => WALLPAPERS.map((w) => ({ ...w, nameKey: w.nameKey })))

const fontSizeLabel = computed(() => {
  const map: Record<number, string> = {
    10: t('shell.settings.ui.fontSmall'),
    12: t('shell.settings.ui.fontNormal'),
    14: t('shell.settings.ui.fontLarge'),
    16: t('shell.settings.ui.fontXl'),
  }
  return map[settingsStore.settings.ui.font_size] ?? t('shell.settings.ui.fontNormal')
})

function setTheme(v: string) {
  settingsStore.settings.ui.theme = v
  void settingsStore.saveSettings({ ui: settingsStore.settings.ui })
}
function setPalette(id: string) {
  settingsStore.settings.ui.theme_palette = id
  void settingsStore.saveSettings({ ui: settingsStore.settings.ui })
}
function setLanguage(e: Event) {
  const v = (e.target as HTMLSelectElement).value
  void i18nStore.setLanguage(v)
}
function setFontSize(n: number) {
  settingsStore.settings.ui.font_size = n
  setupFontSize()
  void settingsStore.saveSettings({ ui: settingsStore.settings.ui })
}
function toggleAnimations() {
  settingsStore.settings.ui.animations_enabled = !(
    settingsStore.settings.ui.animations_enabled !== false
  )
  void settingsStore.saveSettings({ ui: settingsStore.settings.ui })
}
</script>

<style scoped>
.row-sw {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 2px;
}
.row-sw + .row-sw {
  border-top: 1px solid var(--border);
}
.row-sw .lb {
  flex: 1;
  min-width: 0;
}
.row-sw .lb b {
  display: block;
  font-size: calc(12px * var(--ui-scale));
  font-weight: 500;
  color: var(--text-primary);
}
.row-sw .lb span {
  display: block;
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  margin-top: 1px;
}
.fld {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.fld .lb {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-secondary);
}
.fld .hint {
  font-size: calc(10px * var(--ui-scale));
  color: var(--text-tertiary);
}
.shell-inp {
  height: 28px;
  padding: 0 9px;
  background: var(--bg-input);
  border: 1px solid var(--border-input);
  border-radius: var(--radius-input);
  color: var(--text-primary);
  outline: none;
  font-size: calc(12px * var(--ui-scale));
  transition:
    border-color 0.2s,
    box-shadow 0.2s;
}
.shell-inp:focus {
  border-color: var(--border-strong);
  box-shadow: var(--shadow-input-focus);
}
.size-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.size-row input[type='range'] {
  accent-color: var(--color-primary);
  width: 100%;
  cursor: pointer;
}
.size-row .val {
  font-family: var(--font-mono, ui-monospace, 'SF Mono', Menlo, Consolas, monospace);
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-secondary);
  min-width: 34px;
  text-align: right;
}
.pal-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  gap: 8px;
}
.pal-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  border-radius: var(--radius-button);
  border: 1px solid var(--border);
  background: var(--bg-card);
  text-align: left;
  cursor: pointer;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}
.pal-card:hover {
  border-color: var(--border-strong);
}
.pal-card.on {
  border-color: var(--color-primary);
  box-shadow: var(--shadow-input-focus);
}
.pal-sw {
  display: flex;
  height: 22px;
  border-radius: 5px;
  overflow: hidden;
  border: 1px solid var(--border);
}
.pal-sw i {
  flex: 1;
}
.pal-sw i + i {
  border-left: 1px solid var(--border);
}
.pal-card .nm {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-secondary);
}
.pal-card.on .nm {
  color: var(--text-primary);
  font-weight: 500;
}
.mono {
  font-family: var(--font-mono, ui-monospace, 'SF Mono', Menlo, Consolas, monospace);
}
</style>
