<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">设置</h1>
    </header>

    <!-- Connection Status -->
    <div v-if="isConnected" class="px-4 py-2 bg-[var(--mobile-success-muted)] border-b border-[var(--mobile-success)]/20 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-[var(--mobile-success)] shadow-[0_0_6px_rgba(16,185,129,0.5)]"></div>
        <span class="text-[var(--mobile-success)] text-sm">已连接 {{ currentDeviceName }}</span>
      </div>
    </div>
    <div v-else class="px-4 py-2 bg-[var(--mobile-bg-secondary)] border-b border-[var(--mobile-border)] flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-[var(--mobile-text-disabled)]"></div>
        <span class="text-[var(--mobile-text-muted)] text-sm">未连接</span>
      </div>
    </div>

    <!-- Settings List -->
    <div class="flex-1 overflow-auto">
      <!-- Connection Settings -->
      <div class="px-4 py-3 border-b border-[var(--mobile-border)]">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">连接设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">自动重连</span>
            <Toggle v-model="settings.autoReconnect" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">后台保活</span>
            <Toggle v-model="settings.keepAlive" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">重连间隔（秒）</span>
            <input
              v-model.number="settings.reconnectInterval"
              type="number"
              min="1"
              max="60"
              class="w-16 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">默认端口</span>
            <input
              v-model.number="settings.defaultPort"
              type="number"
              min="1"
              max="65535"
              class="w-20 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            />
          </div>
        </div>
      </div>

      <!-- Notification Settings -->
      <div class="px-4 py-3 border-b border-[var(--mobile-border)]">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">通知设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">等待输入提醒</span>
            <Toggle v-model="settings.notifyOnWaiting" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">连接状态提醒</span>
            <Toggle v-model="settings.notifyOnConnection" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">振动反馈</span>
            <Toggle v-model="settings.vibrate" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">后台通知</span>
            <Toggle v-model="settings.notifyInBackground" />
          </div>
        </div>
      </div>

      <!-- Appearance Settings -->
      <div class="px-4 py-3 border-b border-[var(--mobile-border)]">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">外观设置</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">主题</span>
            <select
              v-model="themeMode"
              class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1.5 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            >
              <option value="dark">深色模式</option>
              <option value="light">浅色模式</option>
              <option value="system">跟随系统</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">字体大小</span>
            <select
              v-model="settings.fontSize"
              class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            >
              <option value="small">小</option>
              <option value="medium">中</option>
              <option value="large">大</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">终端缓存数量</span>
            <input
              v-model.number="settings.maxCachedTerminals"
              type="number"
              min="1"
              max="50"
              class="w-16 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            />
          </div>
        </div>
      </div>

      <!-- About -->
      <div class="px-4 py-3">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">关于</h3>

        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-muted)]">版本</span>
            <span class="text-[var(--mobile-text-disabled)]">0.1.0</span>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-muted)]">构建</span>
            <span class="text-[var(--mobile-text-disabled)]">2026-04-30</span>
          </div>

          <button
            class="w-full text-left text-[var(--mobile-text-muted)] py-2 hover:text-[var(--mobile-accent)] transition-colors"
            @click="openGitHub"
          >
            GitHub 仓库 →
          </button>

          <button
            class="w-full text-left text-[var(--mobile-text-muted)] py-2 hover:text-[var(--mobile-accent)] transition-colors"
            @click="checkUpdate"
          >
            检查更新
          </button>
        </div>
      </div>
    </div>

    <!-- Footer Actions -->
    <div class="p-4 border-t border-[var(--mobile-border)] space-y-2 pb-safe">
      <button
        class="w-full bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-input-border)] text-[var(--mobile-text-secondary)] py-3 rounded-xl font-medium hover:border-[var(--mobile-accent)]/40 transition-colors"
        @click="resetSettings"
      >
        重置设置
      </button>
      <button
        class="w-full bg-[var(--mobile-error)]/10 border border-[var(--mobile-error)]/20 text-[var(--mobile-error)] py-3 rounded-xl font-medium hover:bg-[var(--mobile-error)]/20 transition-colors"
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
  fontSize: 'medium',
  maxCachedTerminals: 10,
}

const settings = ref<MobileSettings>({ ...defaultMobileSettings })

// 主题模式 - 直接绑定到 settingsStore
const themeMode = computed({
  get: () => settingsStore.settings.ui.theme,
  set: (value: string) => {
    settingsStore.saveSettings({
      ui: {
        ...settingsStore.settings.ui,
        theme: value
      }
    })
  }
})

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

  settingsStore.saveSettings({
    ui: {
      ...settingsStore.settings.ui,
      terminal_font_size: terminalFontSize,
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
