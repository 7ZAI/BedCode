<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <svg class="w-4 h-4 text-[var(--text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('settings.title') }}</h2>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="settings" />
        <button class="wb-btn-ghost" @click="handleCheckUpdate">
          {{ getUpdateStatusText() }}
        </button>
      </div>
    </div>

    <!-- 内容区：按功能分 section，section 间 24px -->
    <div class="flex-1 overflow-auto px-6 py-6">
      <div class="max-w-3xl mx-auto space-y-6">
        <!-- ==================== APPEARANCE ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.ui.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]">
            <!-- 主题：分段控件 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.appearance.theme') }}</span>
              <div class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0">
                <button
                  v-for="opt in themeOptions"
                  :key="opt.value"
                  class="h-8 px-3 text-xs font-medium transition-colors"
                  :class="themeValue === opt.value
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                    : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'"
                  @click="themeValue = opt.value"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- 主题色板：调色台（色板卡片，切换即时生效） -->
            <div class="px-5 py-3.5 flex items-start justify-between gap-6">
              <div class="flex-shrink-0">
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.appearance.palette') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">{{ t('settings.appearance.paletteDesc') }}</p>
              </div>
              <div class="flex items-start gap-2 flex-wrap justify-end">
                <button
                  v-for="opt in paletteOptions"
                  :key="opt.value"
                  class="w-[84px] rounded-[8px] border p-1.5 transition-colors"
                  :class="paletteValue === opt.value
                    ? 'border-[var(--color-primary)] bg-[var(--color-primary-light)]'
                    : 'border-[var(--border-strong)] hover:border-[var(--text-tertiary)]'"
                  :title="opt.label"
                  @click="paletteValue = opt.value"
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
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.appearance.language') }}</span>
              <div class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0">
                <button
                  v-for="opt in languageOptions"
                  :key="opt.value"
                  class="h-8 px-4 text-xs font-medium transition-colors"
                  :class="currentLanguage === opt.value
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                    : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'"
                  @click="currentLanguage = opt.value"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- 全局字体大小（终端字体在终端设置中独立配置）：小/正常/大/超大 档位间无级滑动 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.appearance.fontSize') }}</span>
              <div class="w-64 flex-shrink-0">
                <div class="flex items-center gap-3">
                  <div class="flex-1">
                    <input
                      type="range"
                      :min="MIN_FONT_SIZE"
                      :max="MAX_FONT_SIZE"
                      step="0.1"
                      :value="settingsStore.settings.ui.font_size"
                      class="w-full h-1 appearance-none bg-[var(--border-strong)] cursor-pointer accent-[var(--color-primary)]"
                      @input="settingsStore.settings.ui.font_size = Number(($event.target as HTMLInputElement).value)"
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
                        @click="settingsStore.settings.ui.font_size = lvl.value"
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

        <!-- ==================== NETWORK ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.network.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]">
            <!-- WebSocket 端口 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.network.websocketPort') }}</span>
              <input
                type="number"
                :value="settingsStore.settings.network.port"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="settingsStore.settings.network.port = Number(($event.target as HTMLInputElement).value)"
              />
            </div>

            <!-- 防止休眠：方角开关 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.network.preventSleep') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">{{ t('settings.network.preventSleepDesc') }}</p>
              </div>
              <button
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
                :class="preventSleep ? 'bg-[var(--color-primary)] border-[var(--color-primary)]' : 'bg-[var(--bg-page)] border-[var(--border-strong)]'"
                role="switch"
                :aria-checked="preventSleep"
                @click="preventSleep = !preventSleep"
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="preventSleep ? 'left-[22px] bg-[var(--color-primary-contrast)]' : 'left-[3px] bg-[var(--border-strong)]'"
                />
              </button>
            </div>
          </div>
        </section>

        <!-- ==================== SESSION ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.session.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]">
            <!-- 默认执行环境：分段控件 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.session.defaultEnvironment') }}</span>
              <div class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0">
                <button
                  v-for="opt in environmentOptions"
                  :key="opt.value"
                  class="h-8 px-4 text-xs font-medium wb-mono transition-colors"
                  :class="defaultEnvironment === opt.value
                    ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                    : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'"
                  @click="defaultEnvironment = opt.value"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- 默认启动命令 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.session.defaultCommand') }}</span>
              <input
                type="text"
                :value="settingsStore.settings.session.default_command || ''"
                class="h-8 w-56 px-2.5 rounded-[6px] wb-mono bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="settingsStore.settings.session.default_command = ($event.target as HTMLInputElement).value"
              />
            </div>
          </div>
        </section>

        <!-- ==================== QR CODE ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.qr.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px]">
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.qr.validity') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">{{ t('settings.qr.validityDesc') }}</p>
              </div>
              <input
                type="number"
                :value="qrTokenTtl"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="qrTokenTtl = Number(($event.target as HTMLInputElement).value)"
                @blur="saveQrTokenTtl"
              />
            </div>
          </div>
        </section>

        <!-- ==================== ABOUT ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.about.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-5 py-4">
            <div class="flex items-center justify-between gap-4">
              <div class="flex items-center gap-2">
                <span class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">BedCode</span>
                <span class="wb-mono text-[var(--text-secondary)]">v{{ appVersion || '—' }}</span>
              </div>
              <div class="flex items-center gap-3">
                <!-- 下载进度 -->
                <template v-if="updateStatus === 'downloading'">
                  <div class="w-32 h-1.5 bg-[var(--border)] overflow-hidden">
                    <div class="h-full bg-[var(--color-primary)] transition-all duration-300" :style="{ width: downloadPercent + '%' }" />
                  </div>
                  <span class="wb-mono text-[var(--text-secondary)]">{{ downloadPercent }}%</span>
                </template>
                <button
                  v-else-if="updateStatus === 'available'"
                  class="wb-btn-primary"
                  @click="handleInstallUpdate"
                >
                  {{ t('settings.about.downloadUpdate') }}
                </button>
                <span v-else-if="updateStatus !== 'idle' && updateStatus !== 'latest' && updateStatus !== 'failed'" class="text-xs text-[var(--text-secondary)]">
                  {{ getUpdateStatusText() }}
                </span>
              </div>
            </div>
            <p v-if="updateStatus === 'failed'" class="mt-2 text-xs text-red-500">{{ errorMessage }}</p>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置视图 — 桌面端设置页面
 * Warm Workbench 风格：分段控件 + 方角开关 + section 分组；支持多主题色板预留
 */
import { onMounted, ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useQrCodeApi } from '@/composables/useTauri'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import i18n from '@/locales'
import { getAppVersion } from '@/composables/useDesktopCommands'
import { useUpdateChecker } from '@/composables/useUpdateChecker'
import { MIN_FONT_SIZE, MAX_FONT_SIZE, NORMAL_FONT_SIZE } from '@/composables/useFontSize'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const qrApi = useQrCodeApi()
const toast = useToast()
const { status: updateStatus, downloadProgress, errorMessage, checkForUpdate, downloadAndInstall, getUpdateStatusText } = useUpdateChecker()

const appVersion = ref('')
const qrTokenTtl = ref(300)

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

const environmentOptions = computed(() => [
  { value: 'windows', label: i18n.global.t('desktop.form.windowsNative') },
  { value: 'wsl2', label: 'WSL2' },
])

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
])

const languageOptions = [
  { value: 'zh-CN', label: '中文' },
  { value: 'en', label: 'English' },
]

// 直接读写 store，主题切换由 useTheme 全局监听即时生效
const themeValue = computed({
  get: () => settingsStore.settings.ui.theme,
  set: (value: string) => { settingsStore.settings.ui.theme = value },
})

// 色板切换由 useTheme 监听 data-palette 即时生效
const paletteValue = computed({
  get: () => settingsStore.settings.ui.theme_palette || 'warm',
  set: (value: string) => { settingsStore.settings.ui.theme_palette = value },
})

const defaultEnvironment = computed({
  get: () => settingsStore.settings.session.default_environment || 'windows',
  set: (value: string) => { settingsStore.settings.session.default_environment = value },
})

const preventSleep = computed({
  get: () => settingsStore.settings.network.prevent_sleep ?? true,
  set: (value: boolean) => { settingsStore.settings.network.prevent_sleep = value },
})

const currentLanguage = computed({
  get: () => settingsStore.settings.ui.language || 'zh-CN',
  set: (value: string) => i18nStore.setLanguage(value),
})

async function loadQrTokenTtl() {
  qrTokenTtl.value = await qrApi.getQrTokenTtl()
}

async function saveQrTokenTtl() {
  const val = Math.max(60, Math.min(3600, qrTokenTtl.value))
  qrTokenTtl.value = val
  await qrApi.setQrTokenTtl(val)
}

// 防抖保存逻辑（防止由保存触发的循环更新）
let saveTimeout: ReturnType<typeof setTimeout> | null = null
let isSaving = false

watch(
  () => settingsStore.settings,
  () => {
    if (isSaving) return
    if (saveTimeout) clearTimeout(saveTimeout)
    saveTimeout = setTimeout(() => {
      isSaving = true
      settingsStore.saveSettings(settingsStore.settings)
      setTimeout(() => { isSaving = false }, 100)
    }, 500)
  },
  { deep: true },
)

onMounted(async () => {
  await settingsStore.loadSettings()
  await loadQrTokenTtl()
  try {
    appVersion.value = await getAppVersion()
  } catch {
    appVersion.value = '—'
  }
})

async function handleCheckUpdate() {
  const update = await checkForUpdate()
  if (!update && updateStatus.value === 'latest') {
    toast.info(i18n.global.t('settings.about.alreadyLatest'))
  } else if (!update && updateStatus.value === 'failed') {
    toast.error(i18n.global.t('settings.about.checkFailed'))
  }
}

async function handleInstallUpdate() {
  await downloadAndInstall()
}

const downloadPercent = computed(() => {
  if (downloadProgress.value.contentLength === 0) return 0
  return Math.round((downloadProgress.value.downloaded / downloadProgress.value.contentLength) * 100)
})
</script>
