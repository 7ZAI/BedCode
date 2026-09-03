<template>
  <SettingsSubPage :title="$t('settings.notification.title')">
    <div class="px-4 py-4 space-y-5">
      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.notification.pushSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.notification.notifyOnWaiting') }}</span>
            <Toggle v-model="settings.notifyOnWaiting" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.notification.notifyOnConnection') }}</span>
            <Toggle v-model="settings.notifyOnConnection" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.notification.notifyInBackground') }}</span>
            <Toggle v-model="settings.notifyInBackground" />
          </div>
        </div>
      </section>

      <section class="space-y-2">
        <h2 class="settings-section-title">{{ $t('settings.notification.feedbackSection') }}</h2>
        <div class="settings-group">
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.notification.vibrate') }}</span>
            <Toggle v-model="settings.vibrate" />
          </div>
          <div class="settings-row">
            <span class="settings-label">{{ $t('settings.notification.soundOnTaskComplete') }}</span>
            <Toggle v-model="settings.soundOnTaskComplete" />
          </div>
        </div>
      </section>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 通知设置二级页面 - 等待输入/连接变化/后台通知/振动/任务完成提示音
 * 状态来自 useMobileSettings 共享单例，变更自动保存；
 * 震动/提示音从关切换到开时各执行一次效果预览（震动一下/响一声）
 */
import { onMounted, watch } from 'vue'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import Toggle from '@/components/Toggle.vue'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { useNotification } from '@/composables/useNotification'

const { settings, loadSettings } = useMobileSettings()
const { previewVibrate, previewSound } = useNotification()

onMounted(loadSettings)

// 仅在用户把开关从关切到开时预览效果；loadSettings 加载时默认值即为 true，
// 不会产生 false→true 的跳变，不会误触发
watch(() => settings.value.vibrate, (now, prev) => {
  if (now && !prev) void previewVibrate()
})
watch(() => settings.value.soundOnTaskComplete, (now, prev) => {
  if (now && !prev) void previewSound()
})
</script>
