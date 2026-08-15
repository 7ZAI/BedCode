<template>
  <SettingsSubPage :title="$t('settings.authentication.title')">
    <div class="px-4 py-4 space-y-5">
      <!-- 优先认证方式 -->
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.authentication.preferredMethod') }}</h2>
        <div class="flex gap-2.5">
          <button
            v-for="method in authMethods"
            :key="method.value"
            class="flex-1 flex items-center justify-center gap-2 px-3 py-3 rounded-xl text-sm font-medium transition-opacity duration-200 active:opacity-80"
            :class="settings.preferredAuthMethod === method.value
              ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] shadow-[0_1px_4px_color-mix(in_srgb,var(--mobile-accent)_40%,transparent)]'
              : 'bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] text-[var(--mobile-text-secondary)]'"
            @click="settings.preferredAuthMethod = method.value"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="method.iconPath" />
            </svg>
            {{ $t(method.labelKey) }}
          </button>
        </div>
        <p class="text-xs text-[var(--mobile-text-muted)]">{{ $t('settings.authentication.degradeHint') }}</p>
      </section>

      <!-- 生物认证密钥 -->
      <section class="settings-group p-4 space-y-3">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2.5">
            <span class="flex-shrink-0 flex items-center justify-center w-9 h-9 rounded-lg bg-[color:color-mix(in_srgb,var(--mobile-accent)_12%,transparent)] text-[var(--mobile-accent)]">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 11c0 3.517-1.009 6.799-2.753 9.571m-3.44-2.04l.054-.09A13.916 13.916 0 008 8a4 4 0 118 0c0 1.017-.07 2.019-.203 3m-2.118 6.844A21.88 21.88 0 0015.171 17m3.839 1.132c.645-2.266.99-4.659.99-7.132A8 8 0 008 4.07M3 15.364c.64-1.319 1-2.8 1-4.364 0-1.457.39-2.823 1.07-4" />
              </svg>
            </span>
            <div class="min-w-0">
              <p class="text-sm font-medium text-[var(--mobile-text-primary)]">{{ $t('settings.authentication.biometricSection') }}</p>
              <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 truncate">{{ $t('settings.authentication.biometricDesc') }}</p>
            </div>
          </div>
          <!-- 状态徽章（仅短状态：已绑定 / 未绑定；长警告文案走下方独立提示行，避免溢出） -->
          <span
            v-if="deviceSupported && !statusError"
            class="flex-shrink-0 inline-flex items-center h-6 px-2.5 rounded-tag text-xs font-medium"
            :class="statusClass"
          >
            {{ statusLabel }}
          </span>
        </div>

        <!-- 警告提示：错误/不支持原因文案较长，独立一行完整换行展示 -->
        <p v-if="statusError" class="text-xs leading-relaxed text-[var(--mobile-error)]">
          ⚠ {{ $t('settings.authentication.statusError') }}
        </p>
        <p v-else-if="!deviceSupported" class="text-xs leading-relaxed text-[var(--mobile-warning)]">
          ⚠ {{ $t(unsupportedReasonKey) }}
        </p>

        <button
          v-if="deviceSupported"
          class="w-full py-3 rounded-xl text-sm font-medium transition-opacity duration-200 active:opacity-80"
          :class="hasKey
            ? 'bg-[var(--mobile-bg-elevated)] border border-[color:color-mix(in_srgb,var(--mobile-error)_40%,transparent)] text-[var(--mobile-error)]'
            : 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'"
          :disabled="busy"
          @click="toggleBind"
        >
          <span v-if="busy" class="inline-flex items-center gap-2">
            <svg class="w-4 h-4 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            {{ $t('common.button.loading') }}
          </span>
          <span v-else>{{ $t(hasKey ? 'settings.authentication.unbind' : 'settings.authentication.bind') }}</span>
        </button>

        <p v-if="!hasKey && deviceSupported" class="text-xs text-[var(--mobile-text-muted)]">{{ $t('settings.authentication.bindHint') }}</p>
      </section>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 认证设置二级页面 - 优先认证方式 + 生物凭证绑定/解绑
 * 优先认证方式存于 useMobileSettings（自动持久化）；
 * 生物凭证状态来自 wsGetBiometricKeyStatus，绑定/解绑需已认证连接。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useToast } from '@/composables/useToast'
import {
  wsGetBiometricKeyStatus,
  wsBindBiometricCredential,
  wsUnbindBiometricCredential,
} from '@/composables/useMobileCommands'

const { t } = useI18n()
const { settings } = useMobileSettings()
const connection = useMobileConnection()
const toast = useToast()

// ==================== 优先认证方式 ====================

interface AuthMethodOption {
  value: 'pairing_code' | 'biometric'
  labelKey: string
  iconPath: string
}

const authMethods: AuthMethodOption[] = [
  {
    value: 'pairing_code',
    labelKey: 'settings.authentication.pairingCode',
    iconPath: 'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z',
  },
  {
    value: 'biometric',
    labelKey: 'settings.authentication.biometric',
    iconPath: 'M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z',
  },
]

// ==================== 生物凭证状态 ====================

const deviceSupported = ref(false)
const deviceReason = ref(-1)
const hasKey = ref(false)
const busy = ref(false)
// 检测调用本身失败（插件未注册/原生异常）时与"设备不支持"区分开，避免误导
const statusError = ref(false)

// BiometricManager 结果码 → 不支持原因文案 key
const unsupportedReasonKey = computed(() => {
  switch (deviceReason.value) {
    case 1:
      return 'settings.authentication.unsupportedUnavailable'
    case 11:
      return 'settings.authentication.unsupportedNotEnrolled'
    case 12:
      return 'settings.authentication.unsupportedNoHardware'
    default:
      return 'settings.authentication.unsupported'
  }
})

const statusLabel = computed(() => (hasKey.value ? t('settings.authentication.bound') : t('settings.authentication.unbound')))

const statusClass = computed(() =>
  hasKey.value
    ? 'bg-[color:color-mix(in_srgb,var(--mobile-success)_12%,transparent)] text-[var(--mobile-success)]'
    : 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-muted)]',
)

async function refreshStatus() {
  try {
    const status = await wsGetBiometricKeyStatus()
    statusError.value = false
    deviceSupported.value = status.deviceSupported
    deviceReason.value = status.deviceReason
    hasKey.value = status.hasKey
  } catch (e) {
    console.warn('[AuthSettings] Failed to load biometric key status:', e)
    statusError.value = true
  }
}

async function toggleBind() {
  if (busy.value) return

  const isConnected =
    connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired'
  if (!isConnected) {
    toast.warning(t('settings.authentication.notConnected'))
    return
  }

  busy.value = true
  try {
    if (hasKey.value) {
      const ok = await wsUnbindBiometricCredential()
      if (ok) {
        toast.success(t('settings.authentication.unbindSuccess'))
      } else {
        toast.error(t('settings.authentication.unbindFailed'))
      }
    } else {
      const ok = await wsBindBiometricCredential()
      if (ok) {
        toast.success(t('settings.authentication.bindSuccess'))
      } else {
        toast.error(t('settings.authentication.bindFailed'))
      }
    }
  } catch (e) {
    console.error('[AuthSettings] Biometric toggle failed:', e)
    // 原生生物识别错误映射为友好 i18n 文案（纯中文，不带 Plugin error: 前缀）
    toast.error(hasKey.value ? t('settings.authentication.unbindFailed') : biometricErrorText(e))
  } finally {
    busy.value = false
    await refreshStatus()
  }
}

// 将原生生物识别错误（系统文案，含 Plugin error: 前缀）映射为友好 i18n 文案
function biometricErrorText(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e)
  // Android 指纹/人脸暂停（失败次数过多）
  if (/尝试次数过多|too many|lockout|paused|暂停/i.test(msg)) {
    return t('settings.authentication.bindLocked')
  }
  // 用户取消弹窗
  if (/cancel|取消/i.test(msg)) {
    return t('settings.authentication.bindCancelled')
  }
  // 生物特征录入变更导致密钥失效（需重新绑定）
  if (/invalidated|失效/i.test(msg)) {
    return t('settings.authentication.bindInvalidated')
  }
  return t('settings.authentication.bindFailed')
}

onMounted(refreshStatus)
</script>
