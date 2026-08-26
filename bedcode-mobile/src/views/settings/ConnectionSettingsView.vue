<template>
  <SettingsSubPage :title="$t('settings.connection.title')">
    <div class="px-4 py-4 space-y-5">
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.reconnectSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.autoReconnect') }}</span>
            <Toggle v-model="settings.autoReconnect" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.keepAlive') }}</span>
            <Toggle v-model="settings.keepAlive" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.reconnectInterval') }}</span>
            <div class="settings-stepper shrink-0">
              <button
                type="button"
                class="settings-stepper-btn"
                :disabled="Number(settings.reconnectInterval) <= 1"
                @click="stepReconnectInterval(-1)"
                :aria-label="t('common.button.decrease')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M20 12H4" /></svg>
              </button>
              <input
                v-model.number="settings.reconnectInterval"
                type="number"
                inputmode="numeric"
                min="1"
                max="60"
                class="settings-number-input"
              />
              <button
                type="button"
                class="settings-stepper-btn"
                :disabled="Number(settings.reconnectInterval) >= 60"
                @click="stepReconnectInterval(1)"
                :aria-label="t('common.button.increase')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M12 4v16m8-8H4" /></svg>
              </button>
            </div>
          </div>
        </div>
      </section>

      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.networkSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.defaultPort') }}</span>
            <div class="settings-stepper shrink-0">
              <button
                type="button"
                class="settings-stepper-btn"
                :disabled="Number(settings.defaultPort) <= 1"
                @click="stepDefaultPort(-1)"
                :aria-label="t('common.button.decrease')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M20 12H4" /></svg>
              </button>
              <input
                v-model.number="settings.defaultPort"
                type="number"
                inputmode="numeric"
                min="1"
                max="65535"
                class="settings-number-input"
              />
              <button
                type="button"
                class="settings-stepper-btn"
                :disabled="Number(settings.defaultPort) >= 65535"
                @click="stepDefaultPort(1)"
                :aria-label="t('common.button.increase')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M12 4v16m8-8H4" /></svg>
              </button>
            </div>
          </div>
        </div>
      </section>
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.linkCryptoSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.linkCryptoMaster') }}</span>
            <Toggle
              :model-value="linkSettings.settings.value.enabled"
              @update:model-value="onToggleLinkEncryption"
            />
          </div>
          <!-- 通道子开关：主开关关时置灰（与桌面端粒度对齐） -->
          <div
            v-for="sub in linkChannelRows"
            :key="sub.channel"
            class="settings-row"
            :class="{ 'opacity-50': !linkSettings.settings.value.enabled }"
          >
            <span class="settings-label">{{ $t(sub.labelKey) }}</span>
            <Toggle
              :model-value="sub.value"
              :disabled="!linkSettings.settings.value.enabled"
              @update:model-value="(v: boolean) => linkSettings.setChannel(sub.channel, v)"
            />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.linkStrictMode') }}</span>
            <Toggle
              :model-value="linkSettings.settings.value.strictMode"
              :disabled="!linkSettings.settings.value.enabled"
              @update:model-value="linkSettings.setStrictMode"
            />
          </div>
          <!-- 对端指纹：人工核对锚点；未配对时展示占位 -->
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.linkPeerFingerprint') }}</span>
            <span class="font-mono text-xs text-[var(--mobile-text-muted)] truncate">{{
              pinnedFingerprint ?? $t('settings.connection.linkNotPaired')
            }}</span>
          </div>
        </div>
        <p class="text-xs text-[var(--mobile-text-muted)] px-1">
          {{ $t('settings.connection.linkCryptoHint') }}
        </p>
      </section>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 连接设置二级页面 - 自动重连、保持连接、重连间隔、默认端口 + 链路加密（issue 08）
 * 状态来自 useMobileSettings 共享单例，变更自动保存
 */
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'
import {
  getPinnedFingerprint,
  useLinkEncryptionSettings,
} from '@/composables/useLinkEncryption'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const toast = useToast()
const { settings, loadSettings } = useMobileSettings()
const linkSettings = useLinkEncryptionSettings()
const pinnedFingerprint = computed(() => getPinnedFingerprint())

/** 通道子开关行元数据（与桌面端三子开关一一对应） */
const linkChannelRows = computed(() => [
  {
    channel: 'http' as const,
    labelKey: 'settings.connection.linkEncryptHttp',
    value: linkSettings.settings.value.encryptHttp,
  },
  {
    channel: 'ws-terminal' as const,
    labelKey: 'settings.connection.linkEncryptWsTerminal',
    value: linkSettings.settings.value.encryptWsTerminal,
  },
  {
    channel: 'ws-event' as const,
    labelKey: 'settings.connection.linkEncryptWsEvent',
    value: linkSettings.settings.value.encryptWsEvent,
  },
])

/**
 * 主开关切换守卫：无 pin 时拒绝开启并引导先配对——加密协商依赖配对期
 * 下发的桌面端身份公钥（pin），无 pin 开关只会产生「开着但永不生效」的半启用态
 */
function onToggleLinkEncryption(next: boolean) {
  if (next && !getPinnedFingerprint()) {
    toast.error(t('settings.connection.linkNeedPairing'))
    return
  }
  linkSettings.setEnabled(next)
}

onMounted(loadSettings)

// ==================== 数字步进 ====================

/** 重连间隔（秒）步进：钳制到 1-60 */
function stepReconnectInterval(delta: number) {
  const next = Number(settings.value.reconnectInterval) + delta
  settings.value.reconnectInterval = Math.max(1, Math.min(60, Number.isFinite(next) ? next : 1))
}

/** 默认端口步进：钳制到 1-65535 */
function stepDefaultPort(delta: number) {
  const next = Number(settings.value.defaultPort) + delta
  settings.value.defaultPort = Math.max(1, Math.min(65535, Number.isFinite(next) ? next : 1))
}
</script>
