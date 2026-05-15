<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-6 py-3 h-12 flex items-center">
      <h2 class="text-lg font-semibold">设置</h2>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-2xl mx-auto space-y-6">
        <!-- Network Settings -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">网络设置</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-gray- dark:text-dark-300 text-sm mb-2">WebSocket 端口</label>
              <input
                v-model.number="settingsStore.settings.network.port"
                type="number"
                class="w-full bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-4 py-2 text-gray-900 dark:text-white focus:border-primary-500 outline-none"
              />
            </div>
          </div>
        </div>

        <!-- Session Defaults -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">会话默认设置</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-gray- dark:text-dark-300 text-sm mb-2">默认执行环境</label>
              <select
                v-model="settingsStore.settings.session.default_environment"
                class="w-full bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-4 py-2 text-gray-900 dark:text-white focus:border-primary-500 outline-none"
              >
                <option value="windows">Windows 原生</option>
                <option value="wsl2">WSL2</option>
              </select>
            </div>
            <div>
              <label class="block text-gray- dark:text-dark-300 text-sm mb-2">默认启动命令</label>
              <input
                v-model="settingsStore.settings.session.default_command"
                type="text"
                class="w-full bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-4 py-2 text-gray-900 dark:text-white focus:border-primary-500 outline-none"
              />
            </div>
          </div>
        </div>

        <!-- QR Code Settings -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">QR 码设置</h3>
          <div class="flex items-center justify-between">
            <div>
              <span class="text-gray- dark:text-dark-200">有效期（秒）</span>
              <p class="text-gray- dark:text-dark-500 text-sm mt-1">QR 码配对令牌的有效期（60-3600 秒）</p>
            </div>
            <input
              v-model.number="qrTokenTtl"
              type="number"
              :min="60"
              :max="3600"
              class="w-24 bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-4 py-2 text-gray-900 dark:text-white text-center focus:border-primary-500 outline-none"
              @blur="saveQrTokenTtl"
            />
          </div>
        </div>

        <!-- UI Settings -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">界面设置</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-gray- dark:text-dark-300 text-sm mb-2">主题</label>
              <select
                v-model="settingsStore.settings.ui.theme"
                class="w-full bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-4 py-2 text-gray-900 dark:text-white focus:border-primary-500 outline-none"
              >
                <option value="light">浅色</option>
                <option value="dark">深色</option>
                <option value="system">跟随系统</option>
              </select>
            </div>
            <div>
              <label class="block text-gray- dark:text-dark-300 text-sm mb-2">终端字体大小</label>
              <div class="flex items-center gap-3">
                <button
                  @click="decrementFontSize"
                  class="w-10 h-10 bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg text-gray-900 dark:text-white hover:bg-dark-600 transition-colors"
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
                  class="w-20 bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg px-4 py-2 text-gray-900 dark:text-white text-center focus:border-primary-500 outline-none"
                />
                <button
                  @click="incrementFontSize"
                  class="w-10 h-10 bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-lg text-gray-900 dark:text-white hover:bg-dark-600 transition-colors"
                  :disabled="settingsStore.settings.ui.terminal_font_size >= 24"
                  :class="{ 'opacity-50 cursor-not-allowed': settingsStore.settings.ui.terminal_font_size >= 24 }"
                >
                  <svg class="w-5 h-5 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                  </svg>
                </button>
                <span class="text-gray- dark:text-dark-400 text-sm">px</span>
              </div>
            </div>
          </div>
        </div>

        <!-- About -->
        <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">关于</h3>
          <div class="text-gray- dark:text-dark-300">
            <p>BedCode</p>
            <p class="text-gray- dark:text-dark-400 text-sm">版本 0.1.0</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { useQrCodeApi } from '@/modules/shared/composables/useTauri'

const settingsStore = useSettingsStore()
const qrApi = useQrCodeApi()

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
