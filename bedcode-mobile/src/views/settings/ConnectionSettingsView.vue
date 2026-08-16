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
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 连接设置二级页面 - 自动重连、保持连接、重连间隔、默认端口
 * 状态来自 useMobileSettings 共享单例，变更自动保存
 */
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'

const { t } = useI18n()
const { settings, loadSettings } = useMobileSettings()

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
