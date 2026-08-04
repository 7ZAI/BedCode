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
            class="flex-1 flex items-center justify-center gap-2 px-3 py-3 rounded-xl border text-sm font-medium transition-all duration-200 active:opacity-80"
            :class="settings.preferredAuthMethod === method.value
              ? 'bg-[var(--mobile-accent)]/15 border-[var(--mobile-accent)] text-[var(--mobile-accent)]'
              : 'bg-[var(--mobile-bg-elevated)] border-[var(--mobile-border)] text-[var(--mobile-text-secondary)]'"
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
            <span class="flex-shrink-0 flex items-center justify-center w-9 h-9 rounded-lg bg-[var(--mobile-accent)]/12 text-[var(--mobile-accent)]">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 11c0 3.517-1.009 6.799-2.753 9.571m-3.44-2.04l.054-.09A13.916 13.916 0 008 8a4 4 0 118 0c0 1.017-.07 2.019-.203 3m-2.118 6.844A21.88 21.88 0 0015.171 17m3.839 1.132c.645-2.266.99-4.659.99-7.132A8 8 0 008 4.07M3 15.364c.64-1.319 1-2.8 1-4.364 0-1.457.39-2.823 1.07-4" />
              </svg>
            </span>
            <div class="min-w-0">
              <p class="text-sm font-medium text-[var(--mobile-text-primary)]">{{ $t('settings.authentication.biometricSection') }}</p>
              <p class="text-xs text-[var(--mobile-text-muted)] mt-0.5 truncate">{{ $t('settings.authentication.biometricDesc') }}</p>
            </div>
          </div>
          <span
            class="flex-shrink-0 inline-flex items-center h-6 px-2.5 rounded-tag text-xs font-medium"
            :class="statusClass"
          >
            {{ statusLabel }}
          </span>
        </div>

        <button
          v-if="deviceSupported"
          class="w-full py-3 rounded-xl text-sm font-medium transition-all duration-200 active:opacity-80"
          :class="hasKey
            ? 'bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-error)]/40 text-[var(--mobile-error)]'
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
        <p v-if="!deviceSupported" class="text-xs text-[var(--mobile-warning)]">{{ $t('settings.authentication.unsupported') }}</p>
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
const hasKey = ref(false)
const busy = ref(false)

const statusLabel = computed(() => {
  if (!deviceSupported.value) return t('settings.authentication.unsupported')
  return hasKey.value ? t('settings.authentication.bound') : t('settings.authentication.unbound')
})

const statusClass = computed(() => {
  if (!deviceSupported.value) return 'bg-[var(--mobile-warning)]/12 text-[var(--mobile-warning)]'
  return hasKey.value
    ? 'bg-[var(--mobile-success)]/12 text-[var(--mobile-success)]'
    : 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-muted)]'
})

async function refreshStatus() {
  try {
    const status = await wsGetBiometricKeyStatus()
    deviceSupported.value = status.deviceSupported
    hasKey.value = status.hasKey
  } catch (e) {
    console.warn('[AuthSettings] Failed to load biometric key status:', e)
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
    toast.error(hasKey.value ? t('settings.authentication.unbindFailed') : t('settings.authentication.bindFailed'))
  } finally {
    busy.value = false
    await refreshStatus()
  }
}

onMounted(refreshStatus)
</script>
