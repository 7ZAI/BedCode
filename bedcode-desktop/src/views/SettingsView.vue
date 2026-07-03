<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-page px-8 h-14 flex items-center">
      <h2 class="text-[var(--font-size-title)] font-semibold text-[var(--text-primary)]">{{ $t('settings.title') }}</h2>
    </header>

    <div class="flex-1 overflow-auto p-6 px-8">
      <div class="max-w-2xl mx-auto space-y-4">
        <!-- Network Settings -->
        <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up">
          <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ $t('settings.network.title') }}</h3>
          <div class="mt-5 space-y-4">
            <Input
              :model-value="settingsStore.settings.network.port"
              type="number"
              :label="$t('settings.network.websocketPort')"
              @update:model-value="settingsStore.settings.network.port = Number($event)"
            />
            <div class="flex items-center justify-between">
              <div>
                <span class="text-[var(--text-primary)] text-sm">{{ $t('settings.network.preventSleep') }}</span>
                <p class="text-[var(--text-tertiary)] text-xs mt-0.5">{{ $t('settings.network.preventSleepDesc') }}</p>
              </div>
              <Toggle
                :model-value="settingsStore.settings.network.prevent_sleep ?? true"
                @update:model-value="settingsStore.settings.network.prevent_sleep = $event"
              />
            </div>
          </div>
        </div>

        <!-- Session Defaults -->
        <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up" style="animation-delay: 50ms">
          <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ $t('settings.session.title') }}</h3>
          <div class="mt-5 grid grid-cols-2 gap-4">
            <Select
              :model-value="settingsStore.settings.session.default_environment || 'windows'"
              :label="$t('settings.session.defaultEnvironment')"
              :options="environmentOptions"
              @update:model-value="settingsStore.settings.session.default_environment = String($event)"
            />
            <Input
              :model-value="settingsStore.settings.session.default_command || ''"
              type="text"
              :label="$t('settings.session.defaultCommand')"
              @update:model-value="settingsStore.settings.session.default_command = String($event)"
            />
          </div>
        </div>

        <!-- QR Code Settings -->
        <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up" style="animation-delay: 100ms">
          <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ $t('settings.qr.title') }}</h3>
          <p class="text-[var(--text-secondary)] text-[13px] mt-1 mb-5">{{ $t('settings.qr.validityDesc') }}</p>
          <div class="flex items-center justify-between">
            <div>
              <span class="text-[var(--text-primary)]">{{ $t('settings.qr.validity') }}</span>
            </div>
            <Input
              :model-value="qrTokenTtl"
              type="number"
              class="w-24"
              @update:model-value="qrTokenTtl = Number($event)"
              @blur="saveQrTokenTtl"
            />
          </div>
        </div>

        <!-- UI Settings -->
        <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up" style="animation-delay: 150ms">
          <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ $t('settings.ui.title') }}</h3>
          <div class="mt-5 space-y-4">
            <Select
              :model-value="settingsStore.settings.ui.theme"
              :label="$t('settings.appearance.theme')"
              :options="themeOptions"
              @update:model-value="settingsStore.settings.ui.theme = String($event)"
            />
            <Select
              :model-value="currentLanguage"
              :label="$t('settings.appearance.language')"
              :options="languageOptions"
              @update:model-value="currentLanguage = String($event)"
            />
            <div>
              <label class="text-xs font-medium text-[var(--text-secondary)] mb-2 block">{{ $t('settings.ui.terminalFontSize') }}</label>
              <div class="flex items-center gap-3">
                <button
                  @click="decrementFontSize"
                  class="w-10 h-10 rounded-btn border border-[var(--border-input)] bg-card text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
                  :disabled="settingsStore.settings.ui.terminal_font_size <= 10"
                  :class="{ 'opacity-50 cursor-not-allowed': settingsStore.settings.ui.terminal_font_size <= 10 }"
                >
                  <svg class="w-5 h-5 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
                  </svg>
                </button>
                <Input
                  :model-value="settingsStore.settings.ui.terminal_font_size"
                  type="number"
                  class="w-20"
                  @update:model-value="settingsStore.settings.ui.terminal_font_size = Number($event)"
                />
                <button
                  @click="incrementFontSize"
                  class="w-10 h-10 rounded-btn border border-[var(--border-input)] bg-card text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
                  :disabled="settingsStore.settings.ui.terminal_font_size >= 24"
                  :class="{ 'opacity-50 cursor-not-allowed': settingsStore.settings.ui.terminal_font_size >= 24 }"
                >
                  <svg class="w-5 h-5 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                  </svg>
                </button>
                <span class="text-[var(--text-secondary)] text-sm">px</span>
              </div>
            </div>
          </div>
        </div>

        <!-- About -->
        <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up" style="animation-delay: 200ms">
          <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ $t('settings.about.title') }}</h3>
          <div class="mt-5 flex items-center justify-between">
            <div class="text-[var(--text-primary)]">
              <p>BedCode</p>
              <p class="text-[var(--text-secondary)] text-sm">{{ $t('common.misc.version') }} {{ appVersion }}</p>
            </div>
            <!-- 检查更新按钮 -->
            <Button
              v-if="updateStatus === 'idle' || updateStatus === 'latest' || updateStatus === 'failed'"
              variant="secondary"
              @click="handleCheckUpdate"
            >
              <template #icon>
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              </template>
              {{ getUpdateStatusText() }}
            </Button>
            <!-- 发现新版本 - 安装按钮 -->
            <Button
              v-else-if="updateStatus === 'available'"
              variant="primary"
              @click="handleInstallUpdate"
            >
              {{ $t('settings.about.downloadUpdate') }}
            </Button>
            <!-- 下载中 -->
            <div v-else-if="updateStatus === 'downloading'" class="flex items-center gap-3">
              <div class="w-32 h-2 bg-[var(--bg-hover)] rounded-full overflow-hidden">
                <div
                  class="h-full bg-brand rounded-full transition-all duration-300"
                  :style="{ width: downloadPercent + '%' }"
                />
              </div>
              <span class="text-sm text-[var(--text-secondary)]">{{ downloadPercent }}%</span>
            </div>
            <!-- 其他状态 -->
            <span v-else class="text-sm text-[var(--text-secondary)]">{{ getUpdateStatusText() }}</span>
          </div>
          <!-- 失败时显示重试 -->
          <p v-if="updateStatus === 'failed'" class="mt-2 text-xs text-red-500">{{ errorMessage }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置视图 - 桌面端设置页面
 * 支持网络、会话、QR码、界面等设置，以及语言切换和更新检查
 */
import { onMounted, ref, watch, computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useQrCodeApi } from '@/composables/useTauri'
import Input from '@/components/Input.vue'
import Select from '@/components/Select.vue'
import Button from '@/components/Button.vue'
import Toggle from '@/components/Toggle.vue'
import i18n from '@/locales'
import { getAppVersion } from '@/composables/useDesktopCommands'
import { useUpdateChecker } from '@/composables/useUpdateChecker'
import { useToast } from '@/composables/useToast'

const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const qrApi = useQrCodeApi()
const toast = useToast()
const { status: updateStatus, downloadProgress, errorMessage, checkForUpdate, downloadAndInstall, getUpdateStatusText } = useUpdateChecker()

const appVersion = ref('')

const environmentOptions = computed(() => [
  { value: 'windows', label: i18n.global.t('desktop.form.windowsNative') },
  { value: 'wsl2', label: 'WSL2' },
])

const themeOptions = computed(() => [
  { value: 'light', label: i18n.global.t('settings.appearance.lightMode') },
  { value: 'dark', label: i18n.global.t('settings.appearance.darkMode') },
  { value: 'system', label: i18n.global.t('settings.appearance.followSystem') },
])

const languageOptions = [
  { value: 'zh-CN', label: '中文' },
  { value: 'en', label: 'English' },
]

const currentLanguage = computed({
  get: () => settingsStore.settings.ui.language || 'zh-CN',
  set: (value: string) => i18nStore.setLanguage(value),
})

const qrTokenTtl = ref(300)

async function loadQrTokenTtl() {
  qrTokenTtl.value = await qrApi.getQrTokenTtl()
}

async function saveQrTokenTtl() {
  const val = Math.max(60, Math.min(3600, qrTokenTtl.value))
  qrTokenTtl.value = val
  await qrApi.setQrTokenTtl(val)
}

function incrementFontSize() {
  if (settingsStore.settings.ui.terminal_font_size < 24) {
    settingsStore.settings.ui.terminal_font_size++
  }
}

function decrementFontSize() {
  if (settingsStore.settings.ui.terminal_font_size > 10) {
    settingsStore.settings.ui.terminal_font_size--
  }
}

let saveTimeout: ReturnType<typeof setTimeout> | null = null
let isSaving = false  // 防止循环保存

watch(
  () => settingsStore.settings,
  () => {
    if (isSaving) return  // 跳过由保存触发的更新
    if (saveTimeout) clearTimeout(saveTimeout)
    saveTimeout = setTimeout(() => {
      isSaving = true
      settingsStore.saveSettings(settingsStore.settings)
      setTimeout(() => { isSaving = false }, 100)  // 100ms 后重置标志
    }, 500)
  },
  { deep: true }
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
  if (update) {
    // 有新版本，UI 自动切换为 "available" 状态，显示安装按钮
  } else if (updateStatus.value === 'latest') {
    toast.info(i18n.global.t('settings.about.alreadyLatest'))
  } else if (updateStatus.value === 'failed') {
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
