<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <h1 class="page-title">{{ $t('settings.title') }}</h1>
    </div>

    <div class="flex-1 overflow-y-auto overflow-x-hidden px-4 pb-8">
      <!-- 分组入口（连接 / 通知 / 安全 / 系统） -->
      <template v-for="group in categoryGroups" :key="group.titleKey">
        <div class="pt-4 pb-1.5">
          <h2 class="settings-section-title">{{ $t(group.titleKey) }}</h2>
        </div>
        <div class="space-y-2">
          <button
            v-for="cat in group.items"
            :key="cat.key"
            class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
            @click="router.push({ name: cat.route })"
          >
            <div class="flex items-center gap-3">
              <span class="icon-chip cat-unified">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" :d="cat.iconPath" />
                </svg>
              </span>
              <span class="flex-1 min-w-0">
                <span class="block text-base font-medium text-[var(--mobile-text-primary)] truncate">{{ $t(cat.labelKey) }}</span>
                <span class="block text-xs mt-0.5 text-[var(--mobile-text-muted)] truncate">{{ $t(cat.descKey) }}</span>
              </span>
              <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </div>
          </button>
        </div>
      </template>

      <!-- Footer Actions -->
      <div class="mt-8 flex flex-col items-center gap-3">
        <button
          class="w-full max-w-xs flex items-center justify-center gap-2 py-3 rounded-xl text-base font-medium transition-colors duration-200 active:scale-[0.98] active:opacity-80"
          style="background: transparent; color: var(--mobile-text-secondary); border: 1px solid var(--mobile-border-hover)"
          @click="resetSettings"
        >
          <svg class="w-[1.125rem] h-[1.125rem] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
          {{ $t('settings.actions.resetSettings') }}
        </button>
        <button
          class="w-full max-w-xs flex items-center justify-center gap-2 py-3 rounded-xl text-base font-medium text-[var(--mobile-error)] bg-[var(--mobile-error-muted)] border danger-action-btn transition-colors duration-200 active:scale-[0.98] active:opacity-80"
          @click="clearData"
        >
          <svg class="w-[1.125rem] h-[1.125rem] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
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
  /** i18n 副标题 key */
  descKey: string
  /** 目标路由名 */
  route: string
  /** SVG path（项目标准线性图标） */
  iconPath: string
}

/** 设置分组：标题 + 组内分类（单一主色，不用彩虹色区分） */
interface SettingsGroup {
  titleKey: string
  items: SettingsCategory[]
}

const categoryGroups: SettingsGroup[] = [
  {
    titleKey: 'settings.groups.connection',
    items: [
      {
        key: 'connection',
        labelKey: 'settings.connection.title',
        descKey: 'settings.connection.subtitle',
        route: 'mobile-settings-connection',
        iconPath: 'M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0',
      },
    ],
  },
  {
    titleKey: 'settings.groups.notification',
    items: [
      {
        key: 'notification',
        labelKey: 'settings.notification.title',
        descKey: 'settings.notification.subtitle',
        route: 'mobile-settings-notifications',
        iconPath: 'M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9',
      },
    ],
  },
  {
    titleKey: 'settings.groups.security',
    items: [
      {
        key: 'authentication',
        labelKey: 'settings.authentication.title',
        descKey: 'settings.authentication.subtitle',
        route: 'mobile-settings-authentication',
        iconPath: 'M12 11c0 3.517-1.009 6.799-2.753 9.571m-3.44-2.04l.054-.09A13.916 13.916 0 008 8a4 4 0 118 0c0 1.017-.07 2.019-.203 3m-2.118 6.844A21.88 21.88 0 0015.171 17m3.839 1.132c.645-2.266.99-4.659.99-7.132A8 8 0 008 4.07M3 15.364c.64-1.319 1-2.8 1-4.364 0-1.457.39-2.823 1.07-4',
      },
    ],
  },
  {
    titleKey: 'settings.groups.system',
    items: [
      {
        key: 'appearance',
        labelKey: 'settings.appearance.title',
        descKey: 'settings.appearance.subtitle',
        route: 'mobile-settings-appearance',
        iconPath: 'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01',
      },
      {
        key: 'plugins',
        labelKey: 'mobile.plugin.title',
        descKey: 'mobile.plugin.subtitle',
        route: 'mobile-plugins',
        iconPath: 'M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z',
      },
      {
        key: 'about',
        labelKey: 'settings.about.title',
        descKey: 'settings.about.subtitle',
        route: 'mobile-settings-about',
        iconPath: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
      },
    ],
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
/* Tailwind 无法对 var() 任意值应用透明度修饰符（border-[var(--x)]/30 不会生成），
   半透明描边与 hover 态用 color-mix 显式实现 */
.danger-action-btn {
  border-color: color-mix(in srgb, var(--mobile-error) 30%, transparent);
}

.danger-action-btn:hover {
  background-color: color-mix(in srgb, var(--mobile-error) 20%, transparent);
  border-color: color-mix(in srgb, var(--mobile-error) 50%, transparent);
}

/* 设置分类图标：统一主色（单一色语言，与主按钮同源） */
.cat-unified {
  color: var(--mobile-accent);
  background-color: var(--mobile-accent-muted);
}

.confirm-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--mobile-overlay-heavy);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-group-bg);
  border: 1px solid var(--mobile-group-border);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: clamp(260px, 320px, 380px);
  text-align: center;
}

.confirm-text {
  font-size: var(--font-size-lg);
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
  font-size: var(--font-size-base);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-group-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
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
