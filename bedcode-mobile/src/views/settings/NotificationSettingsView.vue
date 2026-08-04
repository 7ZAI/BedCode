<template>
  <SettingsSubPage :title="$t('settings.notification.title')">
    <div class="px-4 py-4 space-y-4">
      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.notifyOnWaiting') }}</span>
        <Toggle v-model="settings.notifyOnWaiting" />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.notifyOnConnection') }}</span>
        <Toggle v-model="settings.notifyOnConnection" />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.vibrate') }}</span>
        <Toggle v-model="settings.vibrate" />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.notifyInBackground') }}</span>
        <Toggle v-model="settings.notifyInBackground" />
      </div>

      <div class="flex items-center justify-between">
        <span class="text-sm text-[var(--mobile-text-secondary)]">{{ $t('settings.notification.soundOnTaskComplete') }}</span>
        <Toggle v-model="settings.soundOnTaskComplete" />
      </div>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 通知设置二级页面 - 等待输入/连接变化/振动/后台/任务完成提示音
 * 状态来自 useMobileSettings 共享单例，变更自动保存
 */
import { onMounted } from 'vue'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'

const { settings, loadSettings } = useMobileSettings()

onMounted(loadSettings)
</script>
