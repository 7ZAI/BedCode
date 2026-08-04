<template>
  <SettingsSubPage :title="$t('settings.connection.title')">
    <div class="px-4 py-4 space-y-4">
      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.autoReconnect') }}</span>
        <Toggle v-model="settings.autoReconnect" />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.keepAlive') }}</span>
        <Toggle v-model="settings.keepAlive" />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.reconnectInterval') }}</span>
        <input
          v-model.number="settings.reconnectInterval"
          type="number"
          min="1"
          max="60"
          class="w-16 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
        />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.connection.defaultPort') }}</span>
        <input
          v-model.number="settings.defaultPort"
          type="number"
          min="1"
          max="65535"
          class="w-20 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-2 py-1 text-right text-sm text-[var(--mobile-text-primary)] focus:border-[var(--mobile-accent)] focus:outline-none transition-colors"
        />
      </div>
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
