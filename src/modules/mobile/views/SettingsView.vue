<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ $t('settings.title') }}</h1>
    </header>

    <!-- Settings List -->
    <div class="flex-1 overflow-auto">
      <!-- Connection Settings -->
      <div class="px-4 py-3 border-b border-[var(--mobile-border)]">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">{{ $t('settings.connection.title') }}</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.autoReconnect') }}</span>
            <Toggle v-model="settings.autoReconnect" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.keepAlive') }}</span>
            <Toggle v-model="settings.keepAlive" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.reconnectInterval') }}</span>
            <input
              v-model.number="settings.reconnectInterval"
              type="number"
              min="1"
              max="60"
              class="w-16 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.defaultPort') }}</span>
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
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">{{ $t('settings.notification.title') }}</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.notifyOnWaiting') }}</span>
            <Toggle v-model="settings.notifyOnWaiting" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.notifyOnConnection') }}</span>
            <Toggle v-model="settings.notifyOnConnection" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.vibrate') }}</span>
            <Toggle v-model="settings.vibrate" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.notifyInBackground') }}</span>
            <Toggle v-model="settings.notifyInBackground" />
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.soundOnTaskComplete') }}</span>
            <Toggle v-model="settings.soundOnTaskComplete" />
          </div>
        </div>
      </div>

      <!-- Appearance Settings -->
      <div class="px-4 py-3 border-b border-[var(--mobile-border)]">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">{{ $t('settings.appearance.title') }}</h3>

        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.theme') }}</span>
            <select
              v-model="themeMode"
              class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1.5 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            >
              <option value="dark">{{ $t('settings.appearance.darkMode') }}</option>
              <option value="light">{{ $t('settings.appearance.lightMode') }}</option>
              <option value="system">{{ $t('settings.appearance.followSystem') }}</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.language') }}</span>
            <select
              v-model="currentLanguage"
              class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1.5 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            >
              <option value="zh-CN">中文</option>
              <option value="en">English</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.fontSize') }}</span>
            <select
              v-model="settings.fontSize"
              class="bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-1 text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
            >
              <option value="small">{{ $t('settings.appearance.fontSmall') }}</option>
              <option value="medium">{{ $t('settings.appearance.fontMedium') }}</option>
              <option value="large">{{ $t('settings.appearance.fontLarge') }}</option>
            </select>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-secondary)]">{{ $t('settings.appearance.terminalCacheCount') }}</span>
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
      <div class="px-4 py-3 border-b border-[var(--mobile-border)]">
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 tracking-wider uppercase">{{ $t('settings.about.title') }}</h3>

        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-muted)]">{{ $t('common.misc.version') }}</span>
            <span class="text-[var(--mobile-text-disabled)]">0.1.0</span>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[var(--mobile-text-muted)]">{{ $t('common.misc.build') }}</span>
            <span class="text-[var(--mobile-text-disabled)]">2026-04-30</span>
          </div>

          <button
            class="w-full text-left text-[var(--mobile-text-muted)] py-2 hover:text-[var(--mobile-accent)] transition-colors"
            @click="openGitHub"
          >
            {{ $t('settings.about.githubRepo') }}
          </button>

          <button
            class="w-full text-left text-[var(--mobile-text-muted)] py-2 hover:text-[var(--mobile-accent)] transition-colors"
            @click="checkUpdate"
          >
            {{ $t('settings.about.checkUpdate') }}
          </button>
        </div>
      </div>

      <!-- Footer Actions -->
      <div class="px-4 py-4 space-y-2">
        <button
          class="w-full bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-input-border)] text-[var(--mobile-text-secondary)] py-3 rounded-xl font-medium hover:border-[var(--mobile-accent)]/40 transition-colors"
          @click="resetSettings"
        >
          {{ $t('settings.actions.resetSettings') }}
        </button>
        <button
          class="w-full bg-[var(--mobile-error-muted)] border border-[var(--mobile-error-muted)] text-[var(--mobile-error)] py-3 rounded-xl font-medium hover:bg-[var(--mobile-error)]/20 transition-colors"
          @click="clearData"
        >
          {{ $t('settings.actions.clearAllData') }}
        </button>
      </div>
    </div>

    <!-- Browser Confirm Modal -->
    <Teleport to="body">
      <div v-if="showBrowserConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelOpenBrowser">
        <div class="confirm-modal">
          <p class="confirm-text">{{ $t('settings.browser.confirmOpen') }}</p>
          <p class="confirm-url text-xs text-[var(--mobile-text-muted)] mt-1 mb-4 break-all">{{ pendingUrl }}</p>
          <div class="confirm-buttons">
            <button class="confirm-btn cancel" @click="cancelOpenBrowser">{{ $t('common.button.cancel') }}</button>
            <button class="confirm-btn confirm" @click="confirmOpenBrowser">{{ $t('common.button.open') }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置视图 - 移动端设置页面
 * 支持连接、通知、外观等设置，以及语言切换
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { useForegroundService } from '@/modules/mobile/composables/useForegroundService'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { useI18nStore } from '@/modules/shared/stores/i18n'
import Toggle from '@/modules/shared/components/Toggle.vue'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const connection = useMobileConnection()
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const { startService, stopService, updateNotification } = useForegroundService()

const currentLanguage = computed({
  get: () => settingsStore.settings.ui.language || 'zh-CN',
  set: (value: string) => i18nStore.setLanguage(value),
})

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
  soundOnTaskComplete: boolean
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
  soundOnTaskComplete: true,
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

// ==================== Foreground Service Integration ====================

// keepAlive 开关监听 - 控制前台服务
watch(() => settings.value.keepAlive, async (enabled) => {
  if (enabled && isConnected.value) {
    await startService()
  } else {
    await stopService()
  }
})

// 连接状态变化时更新通知
watch(
  [() => connection.connectionStatus.value, () => connection.activeSessions.value],
  () => {
    if (settings.value.keepAlive) {
      updateNotification()
    }
  },
  { deep: true }
)

// 连接成功时启动服务（如果 keepAlive 开启）
watch(isConnected, async (connected) => {
  if (connected && settings.value.keepAlive) {
    await startService()
  }
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
  if (confirm(t('settings.actions.clearDataConfirm'))) {
    localStorage.clear()
    // In real app, also clear database
    location.reload()
  }
}

// 系统浏览器打开链接的确认弹窗状态
const showBrowserConfirm = ref(false)
const pendingUrl = ref('')

function openGitHub() {
  pendingUrl.value = 'https://github.com/7ZAI/BedCode'
  showBrowserConfirm.value = true
}

async function confirmOpenBrowser() {
  if (pendingUrl.value) {
    try {
      await invoke('open_url_in_browser', { url: pendingUrl.value })
    } catch (e) {
      console.error('Failed to open URL:', e)
    }
  }
  showBrowserConfirm.value = false
  pendingUrl.value = ''
}

function cancelOpenBrowser() {
  showBrowserConfirm.value = false
  pendingUrl.value = ''
}

function checkUpdate() {
  // In real app, check for updates
  alert(t('settings.about.alreadyLatest'))
}

// Auto-save settings
watch(settings, saveSettings, { deep: true })
</script>

<style scoped>
.confirm-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 320px;
  text-align: center;
}

.confirm-text {
  font-size: 1rem;
  color: var(--mobile-text-primary);
  margin: 0;
}

.confirm-url {
  color: var(--mobile-accent);
}

.confirm-buttons {
  display: flex;
  gap: 0.75rem;
  margin-top: 1.25rem;
}

.confirm-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.confirm-btn.confirm {
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
}

.confirm-btn.confirm:hover {
  opacity: 0.9;
}
</style>
