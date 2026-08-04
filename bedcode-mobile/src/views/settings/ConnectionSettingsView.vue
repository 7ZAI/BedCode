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
            <input
              v-model.number="settings.reconnectInterval"
              type="number"
              inputmode="numeric"
              min="1"
              max="60"
              class="settings-number-input shrink-0"
            />
          </div>
        </div>
      </section>

      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.connection.networkSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.connection.defaultPort') }}</span>
            <input
              v-model.number="settings.defaultPort"
              type="number"
              inputmode="numeric"
              min="1"
              max="65535"
              class="settings-number-input shrink-0"
            />
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
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'

const { settings, loadSettings } = useMobileSettings()

onMounted(loadSettings)
</script>
