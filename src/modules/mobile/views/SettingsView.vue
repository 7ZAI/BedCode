<template>
  <div class="h-full flex flex-col bg-[#0a0a0f]">
    <!-- Header -->
    <header class="bg-[#12121a]/90 backdrop-blur-xl border-b border-cyan-500/10 px-4 pb-3" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold text-white tracking-wide">设置</h1>
    </header>

    <!-- Connection Status -->
    <div v-if="isConnected" class="px-4 py-2 bg-emerald-500/10 border-b border-emerald-500/20 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-emerald-500 shadow-[0_0_6px_rgba(16,185,129,0.5)]"></div>
        <span class="text-emerald-400 text-sm">已连接 {{ currentDeviceName }}</span>
      </div>
    </div>
    <div v-else class="px-4 py-2 bg-[#12121a] border-b border-cyan-500/10 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-gray-600"></div>
        <span class="text-gray-500 text-sm">未连接</span>
      </div>
    </div>

    <!-- Settings List -->
    <div class="flex-1 overflow-auto">
      <!-- Connection Settings -->
      <div class="px-4 py-3 border-b border-cyan-500/10">
        <h3 class="text-cyan-400/80 text-sm font-medium mb-3 tracking-wider uppercase">连接设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-gray-300">自动重连</span>
            <Toggle v-model="settings.autoReconnect" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">后台保活</span>
            <Toggle v-model="settings.keepAlive" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">重连间隔（秒）</span>
            <input
              v-model.number="settings.reconnectInterval"
              type="number"
              min="1"
              max="60"
              class="w-16 bg-[#0a0a0f] border border-cyan-500/20 rounded-lg px-2 py-1 text-right text-sm text-white focus:border-cyan-500/50 focus:outline-none transition-colors"
            />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">默认端口</span>
            <input
              v-model.number="settings.defaultPort"
              type="number"
              min="1"
              max="65535"
              class="w-20 bg-[#0a0a0f] border border-cyan-500/20 rounded-lg px-2 py-1 text-right text-sm text-white focus:border-cyan-500/50 focus:outline-none transition-colors"
            />
          </div>
        </div>
      </div>

      <!-- Notification Settings -->
      <div class="px-4 py-3 border-b border-cyan-500/10">
        <h3 class="text-cyan-400/80 text-sm font-medium mb-3 tracking-wider uppercase">通知设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-gray-300">等待输入提醒</span>
            <Toggle v-model="settings.notifyOnWaiting" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">连接状态提醒</span>
            <Toggle v-model="settings.notifyOnConnection" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">振动反馈</span>
            <Toggle v-model="settings.vibrate" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">后台通知</span>
            <Toggle v-model="settings.notifyInBackground" />
          </div>
        </div>
      </div>

      <!-- Appearance Settings -->
      <div class="px-4 py-3 border-b border-cyan-500/10">
        <h3 class="text-cyan-400/80 text-sm font-medium mb-3 tracking-wider uppercase">外观设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-gray-300">深色模式</span>
            <Toggle v-model="settings.darkMode" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">字体大小</span>
            <select
              v-model="settings.fontSize"
              class="bg-[#0a0a0f] border border-cyan-500/20 rounded-lg px-3 py-1 text-sm text-white focus:border-cyan-500/50 focus:outline-none transition-colors"
            >
              <option value="small">小</option>
              <option value="medium">中</option>
              <option value="large">大</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-300">终端缓存数量</span>
            <input
              v-model.number="settings.maxCachedTerminals"
              type="number"
              min="1"
              max="50"
              class="w-16 bg-[#0a0a0f] border border-cyan-500/20 rounded-lg px-2 py-1 text-right text-sm text-white focus:border-cyan-500/50 focus:outline-none transition-colors"
            />
          </div>
        </div>
      </div>

      <!-- About -->
      <div class="px-4 py-3">
        <h3 class="text-cyan-400/80 text-sm font-medium mb-3 tracking-wider uppercase">关于</h3>

        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-gray-400">版本</span>
            <span class="text-gray-600">0.1.0</span>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-gray-400">构建</span>
            <span class="text-gray-600">2026-04-30</span>
          </div>

          <button
            class="w-full text-left text-gray-400 py-2 hover:text-cyan-400 transition-colors"
            @click="openGitHub"
          >
            GitHub 仓库 →
          </button>

          <button
            class="w-full text-left text-gray-400 py-2 hover:text-cyan-400 transition-colors"
            @click="checkUpdate"
          >
            检查更新
          </button>
        </div>
      </div>
    </div>

    <!-- Footer Actions -->
    <div class="p-4 border-t border-cyan-500/10 space-y-2 pb-safe">
      <button
        class="w-full bg-[#12121a] border border-cyan-500/20 text-gray-300 py-3 rounded-xl font-medium hover:border-cyan-500/40 transition-colors"
        @click="resetSettings"
      >
        重置设置
      </button>
      <button
        class="w-full bg-red-500/10 border border-red-500/20 text-red-400 py-3 rounded-xl font-medium hover:bg-red-500/20 transition-colors"
        @click="clearData"
      >
        清除所有数据
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import Toggle from '@/modules/shared/components/Toggle.vue'
import { invoke } from '@tauri-apps/api/core'

const connection = useMobileConnection()
const settingsStore = useSettingsStore()

// 使用统一的连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前设备名称
const currentDeviceName = computed(() => connection.currentDevice.value?.name || '')

// 移动端本地设置（用于 UI 控制）
interface MobileSettings {
  autoReconnect: boolean
  keepAlive: boolean
  reconnectInterval: number
  defaultPort: number
  notifyOnWaiting: boolean
  notifyOnConnection: boolean
  notifyInBackground: boolean
  vibrate: boolean
  darkMode: boolean
  fontSize: 'small' | 'medium' | 'large'
  maxCachedTerminals: number
}

const defaultMobileSettings: MobileSettings = {
  autoReconnect: true,
  keepAlive: true,
  reconnectInterval: 5,
  defaultPort: 8765,
  notifyOnWaiting: true,
  notifyOnConnection: true,
  notifyInBackground: true,
  vibrate: true,
  darkMode: true,
  fontSize: 'medium',
  maxCachedTerminals: 10,
}

const settings = ref<MobileSettings>({ ...defaultMobileSettings })

// 字体大小映射
const fontSizeMap = {
  small: 12,
  medium: 14,
  large: 16
}

onMounted(async () => {
  // 先等待 settingsStore 加载完成
  await settingsStore.loadSettings()

  // 加载已保存的设置
  const saved = localStorage.getItem('mobile-settings')
  if (saved) {
    try {
      const parsed = JSON.parse(saved)
      settings.value = { ...defaultMobileSettings, ...parsed }
    } catch (e) {
      console.error('Failed to load settings:', e)
    }
  }

  // 尝试从后端加载移动端设置并同步
  try {
    const dbSettings = await invoke<Array<{ key: string; value: string }>>('get_all_db_settings')
    for (const s of dbSettings) {
      if (s.key.startsWith('mobile.')) {
        const settingKey = s.key.replace('mobile.', '')
        const value = s.value === 'true' ? true : s.value === 'false' ? false : isNaN(Number(s.value)) ? s.value : Number(s.value)
        ;(settings.value as any)[settingKey] = value
      }
    }
  } catch {
    // Backend may not be available
  }

  // 同步到 settingsStore（使设置生效）
  syncToSettingsStore()
})

// 将移动端设置同步到全局 settingsStore
function syncToSettingsStore() {
  // 字体大小映射到终端字体大小
  const terminalFontSize = fontSizeMap[settings.value.fontSize]

  // 深色模式映射到 theme（darkMode: true = dark, false = light）
  const theme = settings.value.darkMode ? 'dark' : 'light'

  settingsStore.saveSettings({
    ui: {
      ...settingsStore.settings.ui,
      terminal_font_size: terminalFontSize,
      theme: theme
    }
  })
}

function saveSettings() {
  // 保存到本地存储
  localStorage.setItem('mobile-settings', JSON.stringify(settings.value))

  // 同步到全局 settingsStore（使设置生效）
  syncToSettingsStore()

  // 同时保存到后端数据库
  for (const [key, value] of Object.entries(settings.value)) {
    invoke('set_db_setting', {
      key: `mobile.${key}`,
      value: String(value),
    }).catch(() => {})
  }
}

function resetSettings() {
  settings.value = { ...defaultMobileSettings }
  saveSettings()
}

function clearData() {
  if (confirm('确定要清除所有数据吗？这将删除所有配对设备、快捷指令和历史记录。')) {
    localStorage.clear()
    // In real app, also clear database
    location.reload()
  }
}

function openGitHub() {
  window.open('https://github.com/your-repo/bedcode', '_blank')
}

function checkUpdate() {
  // In real app, check for updates
  alert('已是最新版本')
}

// Auto-save settings
watch(settings, saveSettings, { deep: true })
</script>
