<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3">
      <h1 class="text-lg font-semibold">设置</h1>
    </header>

    <!-- Settings List -->
    <div class="flex-1 overflow-auto">
      <!-- Connection Settings -->
      <div class="px-4 py-3 border-b border-dark-800">
        <h3 class="text-dark-400 text-sm font-medium mb-3">连接设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span>自动重连</span>
            <Toggle v-model="settings.autoReconnect" />
          </div>

          <div class="flex items-center justify-between">
            <span>后台保活</span>
            <Toggle v-model="settings.keepAlive" />
          </div>

          <div class="flex items-center justify-between">
            <span>重连间隔（秒）</span>
            <input
              v-model.number="settings.reconnectInterval"
              type="number"
              min="1"
              max="60"
              class="w-16 bg-dark-700 border border-dark-600 rounded-lg px-2 py-1 text-right text-sm"
            />
          </div>
        </div>
      </div>

      <!-- Notification Settings -->
      <div class="px-4 py-3 border-b border-dark-800">
        <h3 class="text-dark-400 text-sm font-medium mb-3">通知设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span>等待输入提醒</span>
            <Toggle v-model="settings.notifyOnWaiting" />
          </div>

          <div class="flex items-center justify-between">
            <span>连接状态提醒</span>
            <Toggle v-model="settings.notifyOnConnection" />
          </div>

          <div class="flex items-center justify-between">
            <span>振动反馈</span>
            <Toggle v-model="settings.vibrate" />
          </div>
        </div>
      </div>

      <!-- Appearance Settings -->
      <div class="px-4 py-3 border-b border-dark-800">
        <h3 class="text-dark-400 text-sm font-medium mb-3">外观设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span>深色模式</span>
            <Toggle v-model="settings.darkMode" disabled />
          </div>

          <div class="flex items-center justify-between">
            <span>字体大小</span>
            <select
              v-model="settings.fontSize"
              class="bg-dark-700 border border-dark-600 rounded-lg px-3 py-1 text-sm"
            >
              <option value="small">小</option>
              <option value="medium">中</option>
              <option value="large">大</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span>终端输出模式</span>
            <select
              v-model="settings.defaultOutputMode"
              class="bg-dark-700 border border-dark-600 rounded-lg px-3 py-1 text-sm"
            >
              <option value="enhanced">增强</option>
              <option value="raw">原始</option>
            </select>
          </div>
        </div>
      </div>

      <!-- About -->
      <div class="px-4 py-3">
        <h3 class="text-dark-400 text-sm font-medium mb-3">关于</h3>

        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-dark-300">版本</span>
            <span class="text-dark-500">0.1.0</span>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-dark-300">构建</span>
            <span class="text-dark-500">2026-04-30</span>
          </div>

          <button
            class="w-full text-left text-dark-300 py-2"
            @click="openGitHub"
          >
            GitHub 仓库 →
          </button>

          <button
            class="w-full text-left text-dark-300 py-2"
            @click="checkUpdate"
          >
            检查更新
          </button>
        </div>
      </div>
    </div>

    <!-- Footer Actions -->
    <div class="p-4 border-t border-dark-700 space-y-2">
      <button
        class="w-full bg-dark-700 text-dark-200 py-3 rounded-xl font-medium"
        @click="resetSettings"
      >
        重置设置
      </button>
      <button
        class="w-full bg-red-900/50 text-red-400 py-3 rounded-xl font-medium"
        @click="clearData"
      >
        清除所有数据
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Toggle from '@/components/common/Toggle.vue'

interface Settings {
  autoReconnect: boolean
  keepAlive: boolean
  reconnectInterval: number
  notifyOnWaiting: boolean
  notifyOnConnection: boolean
  vibrate: boolean
  darkMode: boolean
  fontSize: 'small' | 'medium' | 'large'
  defaultOutputMode: 'enhanced' | 'raw'
}

const defaultSettings: Settings = {
  autoReconnect: true,
  keepAlive: true,
  reconnectInterval: 5,
  notifyOnWaiting: true,
  notifyOnConnection: true,
  vibrate: true,
  darkMode: true,
  fontSize: 'medium',
  defaultOutputMode: 'enhanced'
}

const settings = ref<Settings>({ ...defaultSettings })

onMounted(() => {
  // Load settings from storage
  const saved = localStorage.getItem('mobile-settings')
  if (saved) {
    try {
      const parsed = JSON.parse(saved)
      settings.value = { ...defaultSettings, ...parsed }
    } catch (e) {
      console.error('Failed to load settings:', e)
    }
  }
})

function saveSettings() {
  localStorage.setItem('mobile-settings', JSON.stringify(settings.value))
}

function resetSettings() {
  settings.value = { ...defaultSettings }
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
import { watch } from 'vue'
watch(settings, saveSettings, { deep: true })
</script>
