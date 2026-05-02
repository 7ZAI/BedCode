<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-6 py-3 h-12 flex items-center">
      <h2 class="text-lg font-semibold">设置</h2>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-2xl mx-auto space-y-6">
        <!-- Network Settings -->
        <div class="bg-dark-800 rounded-lg border border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">网络设置</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-dark-300 text-sm mb-2">WebSocket 端口</label>
              <input
                v-model.number="settingsStore.settings.network.port"
                type="number"
                class="w-full bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white focus:border-primary-500 outline-none"
              />
            </div>
            <div class="flex items-center justify-between">
              <span class="text-dark-300">启用设备发现 (mDNS)</span>
              <button
                @click="settingsStore.settings.network.enable_discovery = !settingsStore.settings.network.enable_discovery"
                :class="[
                  'w-12 h-6 rounded-full transition-colors',
                  settingsStore.settings.network.enable_discovery ? 'bg-primary-600' : 'bg-dark-600'
                ]"
              >
                <div
                  :class="[
                    'w-5 h-5 rounded-full bg-white transition-transform',
                    settingsStore.settings.network.enable_discovery ? 'translate-x-6' : 'translate-x-1'
                  ]"
                ></div>
              </button>
            </div>
          </div>
        </div>

        <!-- Session Defaults -->
        <div class="bg-dark-800 rounded-lg border border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">会话默认设置</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-dark-300 text-sm mb-2">默认执行环境</label>
              <select
                v-model="settingsStore.settings.session.default_environment"
                class="w-full bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white focus:border-primary-500 outline-none"
              >
                <option value="windows">Windows 原生</option>
                <option value="wsl2">WSL2</option>
              </select>
            </div>
            <div>
              <label class="block text-dark-300 text-sm mb-2">默认启动命令</label>
              <input
                v-model="settingsStore.settings.session.default_command"
                type="text"
                class="w-full bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white focus:border-primary-500 outline-none"
              />
            </div>
          </div>
        </div>

        <!-- UI Settings -->
        <div class="bg-dark-800 rounded-lg border border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">界面设置</h3>
          <div class="space-y-4">
            <div>
              <label class="block text-dark-300 text-sm mb-2">主题</label>
              <select
                v-model="settingsStore.settings.ui.theme"
                class="w-full bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white focus:border-primary-500 outline-none"
              >
                <option value="light">浅色</option>
                <option value="dark">深色</option>
                <option value="system">跟随系统</option>
              </select>
            </div>
            <div>
              <label class="block text-dark-300 text-sm mb-2">终端字体大小</label>
              <div class="flex items-center gap-3">
                <button
                  @click="decrementFontSize"
                  class="w-10 h-10 bg-dark-700 border border-dark-600 rounded-lg text-white hover:bg-dark-600 transition-colors"
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
                  class="w-20 bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white text-center focus:border-primary-500 outline-none"
                />
                <button
                  @click="incrementFontSize"
                  class="w-10 h-10 bg-dark-700 border border-dark-600 rounded-lg text-white hover:bg-dark-600 transition-colors"
                  :disabled="settingsStore.settings.ui.terminal_font_size >= 24"
                  :class="{ 'opacity-50 cursor-not-allowed': settingsStore.settings.ui.terminal_font_size >= 24 }"
                >
                  <svg class="w-5 h-5 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                  </svg>
                </button>
                <span class="text-dark-400 text-sm">px</span>
              </div>
            </div>
          </div>
        </div>

        <!-- About -->
        <div class="bg-dark-800 rounded-lg border border-dark-700 p-6">
          <h3 class="text-lg font-medium mb-4">关于</h3>
          <div class="text-dark-300">
            <p>BedCode</p>
            <p class="text-dark-400 text-sm">版本 0.1.0</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useSettingsStore } from '@/stores/settings'

const settingsStore = useSettingsStore()

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

onMounted(async () => {
  await settingsStore.loadSettings()
})
</script>
