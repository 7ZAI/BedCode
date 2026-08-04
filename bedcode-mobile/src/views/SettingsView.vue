<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="flex-shrink-0 bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ $t('settings.title') }}</h1>
    </header>

    <div class="flex-1 overflow-y-auto overflow-x-hidden">
      <!-- Category Entries -->
      <nav class="p-4 space-y-2.5">
        <button
          v-for="cat in categories"
          :key="cat.key"
          class="w-full flex items-center gap-3 px-4 py-3 bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl hover:border-[var(--mobile-border-hover)] active:opacity-80 transition-all duration-200"
          @click="router.push({ name: cat.route })"
        >
          <span class="cat-icon" :class="cat.iconClass">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="cat.iconPath" />
            </svg>
          </span>
          <span class="flex-1 min-w-0 text-left text-[0.9375rem] font-medium text-[var(--mobile-text-primary)] truncate">{{ $t(cat.labelKey) }}</span>
          <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </nav>

      <!-- Footer Actions -->
      <div class="px-4 pb-4 space-y-2">
        <button
          class="w-full bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-input-border)] text-sm text-[var(--mobile-text-secondary)] py-3 rounded-xl font-medium hover:border-[var(--mobile-accent)]/40 transition-colors"
          @click="resetSettings"
        >
          {{ $t('settings.actions.resetSettings') }}
        </button>
        <button
          class="w-full bg-[var(--mobile-error-muted)] border border-[var(--mobile-error-muted)] text-[var(--mobile-error)] text-sm py-3 rounded-xl font-medium hover:bg-[var(--mobile-error)]/20 transition-colors"
          @click="clearData"
        >
          {{ $t('settings.actions.clearAllData') }}
        </button>
      </div>
    </div>

    <!-- Confirm Dialog (Reset / Clear Data) -->
    <Teleport to="body">
      <Transition name="center-modal">
      <div v-if="showConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelConfirm">
        <div class="confirm-modal modal-panel">
          <p class="confirm-text">{{ confirmMessage }}</p>
          <div class="confirm-buttons">
            <button class="confirm-btn cancel" @click="cancelConfirm">{{ $t('common.button.cancel') }}</button>
            <button class="confirm-btn confirm danger" @click="executeConfirm">{{ $t('common.button.confirm') }}</button>
          </div>
        </div>
      </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置主页 - 分类入口导航（TikTok 风格）
 *
 * 主页只展示设置分类入口，真正的设置项在各分类二级页面。
 * 设置状态由 useMobileSettings 模块级单例共享，二级页面与主页数据一致。
 * 前台服务相关的 watcher 保留在主页（主页常驻于滑动容器中，始终挂载）。
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useForegroundService } from '@/composables/useForegroundService'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { clearAuthCredentials } from '@/composables/useMobileCommands'
import { clearAllTasks } from '@/composables/usePresetTasks'

const { t } = useI18n()
const router = useRouter()
const connection = useMobileConnection()
const { settings, loadSettings, resetSettings: applyResetSettings } = useMobileSettings()
const { startService, stopService, updateNotification } = useForegroundService()

// ==================== Category Entries ====================

/** 设置分类入口配置 */
interface SettingsCategory {
  key: string
  /** i18n 标签 key */
  labelKey: string
  /** 目标路由名 */
  route: string
  /** SVG path（项目标准线性图标） */
  iconPath: string
  /** 图标配色类（scoped 样式） */
  iconClass: string
}

const categories: SettingsCategory[] = [
  {
    key: 'connection',
    labelKey: 'settings.connection.title',
    route: 'mobile-settings-connection',
    iconPath: 'M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0',
    iconClass: 'cat-connection',
  },
  {
    key: 'notification',
    labelKey: 'settings.notification.title',
    route: 'mobile-settings-notifications',
    iconPath: 'M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9',
    iconClass: 'cat-notification',
  },
  {
    key: 'authentication',
    labelKey: 'settings.authentication.title',
    route: 'mobile-settings-authentication',
    iconPath: 'M12 11c0 3.517-1.009 6.799-2.753 9.571m-3.44-2.04l.054-.09A13.916 13.916 0 008 8a4 4 0 118 0c0 1.017-.07 2.019-.203 3m-2.118 6.844A21.88 21.88 0 0015.171 17m3.839 1.132c.645-2.266.99-4.659.99-7.132A8 8 0 008 4.07M3 15.364c.64-1.319 1-2.8 1-4.364 0-1.457.39-2.823 1.07-4',
    iconClass: 'cat-authentication',
  },
  {
    key: 'appearance',
    labelKey: 'settings.appearance.title',
    route: 'mobile-settings-appearance',
    iconPath: 'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01',
    iconClass: 'cat-appearance',
  },
  {
    key: 'plugins',
    labelKey: 'mobile.plugin.title',
    route: 'mobile-plugins',
    iconPath: 'M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z',
    iconClass: 'cat-plugins',
  },
  {
    key: 'about',
    labelKey: 'settings.about.title',
    route: 'mobile-settings-about',
    iconPath: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
    iconClass: 'cat-about',
  },
]

// 使用统一的连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

onMounted(() => {
  loadSettings()
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

// ==================== Actions ====================

function resetSettings() {
  showConfirmDialog(
    t('settings.actions.resetSettingsConfirm'),
    applyResetSettings
  )
}

async function clearData() {
  showConfirmDialog(
    t('settings.actions.clearDataConfirm'),
    async () => {
      try {
        // 1. 断开当前连接
        if (isConnected.value) {
          await connection.disconnect()
        }
        // 停止前台服务
        await stopService()
      } catch (e) {
        console.warn('[Settings] Disconnect/stopService failed, continuing cleanup:', e)
      }

      // 2. 清除预设任务
      clearAllTasks()

      // 3. 清除连接历史和配对设备
      connection.clearConnectionHistory()
      connection.clearPairedDevices()
      connection.clearSessionConfigs()
      connection.clearActiveSessions()

      // 4. 清除认证凭据
      clearAuthCredentials()
      connection.clearCredentials()

      // 5. 清除所有 localStorage
      localStorage.clear()

      // 6. 重新加载页面
      location.reload()
    }
  )
}

// ==================== Confirm Dialog ====================

const showConfirm = ref(false)
const confirmMessage = ref('')
let confirmCallback: (() => Promise<void>) | null = null

function showConfirmDialog(message: string, onConfirm: () => Promise<void>) {
  confirmMessage.value = message
  confirmCallback = onConfirm
  showConfirm.value = true
}

function cancelConfirm() {
  showConfirm.value = false
  confirmMessage.value = ''
  confirmCallback = null
}

async function executeConfirm() {
  const callback = confirmCallback
  confirmCallback = null
  showConfirm.value = false
  confirmMessage.value = ''
  if (callback) {
    try {
      await callback()
    } catch (e) {
      console.error('[Settings] Confirm action failed:', e)
    }
  }
}
</script>

<style scoped>
/* 分类入口图标容器 */
.cat-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 0.75rem;
  flex-shrink: 0;
}

/* 分类图标配色：装饰性强调色，明暗主题下保持一致 */
.cat-connection {
  color: var(--mobile-accent);
  background-color: color-mix(in srgb, var(--mobile-accent) 14%, transparent);
}

.cat-notification {
  color: var(--mobile-warning);
  background-color: color-mix(in srgb, var(--mobile-warning) 14%, transparent);
}

/* 认证设置：生物识别指纹，与 --mobile-success 同源 */
.cat-authentication {
  color: var(--mobile-success);
  background-color: color-mix(in srgb, var(--mobile-success) 14%, transparent);
}

.cat-appearance {
  /* Dracula 紫，与 --mobile-shortcut-color 同源 */
  color: #bd93f9;
  background-color: color-mix(in srgb, #bd93f9 14%, transparent);
}

.cat-plugins {
  color: var(--mobile-success);
  background-color: color-mix(in srgb, var(--mobile-success) 14%, transparent);
}

.cat-about {
  color: var(--mobile-text-muted);
  background-color: var(--mobile-bg-elevated);
}

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
  z-index: 50;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: clamp(260px, 320px, 380px);
  text-align: center;
}

.confirm-text {
  font-size: 1rem;
  color: var(--mobile-text-primary);
  margin: 0;
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

.confirm-btn.confirm.danger {
  background: var(--mobile-error);
  color: white;
}

.confirm-btn.confirm.danger:hover {
  opacity: 0.9;
}
</style>
