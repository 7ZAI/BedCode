<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-slate-200 dark:border-dark-700 px-6 py-3 h-12 flex items-center shadow-sm dark:shadow-none">
      <h2 class="text-lg font-semibold">{{ $t('settings.title') }}</h2>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-2xl mx-auto space-y-6">
        <!-- Network Settings -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 p-6 shadow-sm dark:shadow-none">
          <h3 class="text-lg font-medium mb-4">{{ $t('settings.network.title') }}</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-slate-700 dark:text-dark-300 text-sm mb-2">{{ $t('settings.network.websocketPort') }}</label>
              <input
                v-model.number="settingsStore.settings.network.port"
                type="number"
                class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
              />
            </div>
          </div>
        </div>

        <!-- Session Defaults -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 p-6 shadow-sm dark:shadow-none">
          <h3 class="text-lg font-medium mb-4">{{ $t('settings.session.title') }}</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-slate-700 dark:text-dark-300 text-sm mb-2">{{ $t('settings.session.defaultEnvironment') }}</label>
              <select
                v-model="settingsStore.settings.session.default_environment"
                class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
              >
                <option value="windows">{{ $t('desktop.form.windowsNative') }}</option>
                <option value="wsl2">WSL2</option>
              </select>
            </div>
            <div>
              <label class="block text-slate-700 dark:text-dark-300 text-sm mb-2">{{ $t('settings.session.defaultCommand') }}</label>
              <input
                v-model="settingsStore.settings.session.default_command"
                type="text"
                class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
              />
            </div>
          </div>
        </div>

        <!-- QR Code Settings -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 p-6 shadow-sm dark:shadow-none">
          <h3 class="text-lg font-medium mb-4">{{ $t('settings.qr.title') }}</h3>
          <div class="flex items-center justify-between">
            <div>
              <span class="text-slate-800 dark:text-dark-200">{{ $t('settings.qr.validity') }}</span>
              <p class="text-slate-500 dark:text-dark-500 text-sm mt-1">{{ $t('settings.qr.validityDesc') }}</p>
            </div>
            <input
              v-model.number="qrTokenTtl"
              type="number"
              :min="60"
              :max="3600"
              class="w-24 bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white text-center focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
              @blur="saveQrTokenTtl"
            />
          </div>
        </div>

        <!-- UI Settings -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 p-6 shadow-sm dark:shadow-none">
          <h3 class="text-lg font-medium mb-4">{{ $t('settings.ui.title') }}</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-slate-700 dark:text-dark-300 text-sm mb-2">{{ $t('settings.appearance.theme') }}</label>
              <select
                v-model="settingsStore.settings.ui.theme"
                class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
              >
                <option value="light">{{ $t('settings.appearance.lightMode') }}</option>
                <option value="dark">{{ $t('settings.appearance.darkMode') }}</option>
                <option value="system">{{ $t('settings.appearance.followSystem') }}</option>
              </select>
            </div>
            <div>
              <label class="block text-slate-700 dark:text-dark-300 text-sm mb-2">{{ $t('settings.appearance.language') }}</label>
              <select
                v-model="currentLanguage"
                class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
              >
                <option value="zh-CN">中文</option>
                <option value="en">English</option>
              </select>
            </div>
            <div>
              <label class="block text-slate-700 dark:text-dark-300 text-sm mb-2">{{ $t('settings.ui.terminalFontSize') }}</label>
              <div class="flex items-center gap-3">
                <button
                  @click="decrementFontSize"
                  class="w-10 h-10 bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg text-slate-900 dark:text-white hover:bg-slate-100 dark:hover:bg-dark-600 transition-colors shadow-xs dark:shadow-none"
                  :disabled="settingsStore.settings.ui.terminal_font_size <= 10"
                  :class="{ 'opacity-50 cursor-not-allowed': settingsStore.settings.ui.terminal_font_size <= 10 }"
                >
                  <svg class="w-5 h-5 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
                  </svg>
                </button>
                <input
                  v-model.number="settingsStore.settings.ui.terminal_font_size"
                  type="number"
                  min="10"
                  max="24"
                  class="w-20 bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-4 py-2 text-slate-900 dark:text-white text-center focus:border-primary-500 outline-none shadow-xs dark:shadow-none"
                />
                <button
                  @click="incrementFontSize"
                  class="w-10 h-10 bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg text-slate-900 dark:text-white hover:bg-slate-100 dark:hover:bg-dark-600 transition-colors shadow-xs dark:shadow-none"
                  :disabled="settingsStore.settings.ui.terminal_font_size >= 24"
                  :class="{ 'opacity-50 cursor-not-allowed': settingsStore.settings.ui.terminal_font_size >= 24 }"
                >
                  <svg class="w-5 h-5 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                  </svg>
                </button>
                <span class="text-slate-500 dark:text-dark-400 text-sm">px</span>
              </div>
            </div>
          </div>
        </div>

        <!-- About -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 p-6 shadow-sm dark:shadow-none">
          <h3 class="text-lg font-medium mb-4">{{ $t('settings.about.title') }}</h3>
          <div class="text-slate-700 dark:text-dark-300">
            <p>BedCode</p>
            <p class="text-slate-500 dark:text-dark-400 text-sm">{{ $t('common.misc.version') }} 0.1.0</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置视图 - 桌面端设置页面
 * 支持网络、会话、QR码、界面等设置，以及语言切换
 */
import { onMounted, ref, watch, computed } from 'vue'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { useI18nStore } from '@/modules/shared/stores/i18n'
import { useQrCodeApi } from '@/modules/shared/composables/useTauri'

const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const qrApi = useQrCodeApi()

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
})
</script>
