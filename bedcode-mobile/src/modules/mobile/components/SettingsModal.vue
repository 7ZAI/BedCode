<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl w-full max-w-sm p-5 shadow-xl">
        <div class="flex items-center justify-between mb-5">
          <span class="font-semibold text-[var(--mobile-text-primary)] text-lg">{{ t('mobile.inputAssistant.title') }}</span>
          <button
            class="p-1.5 rounded-lg hover:bg-[var(--mobile-accent-muted)] transition-colors"
            @click="emit('close')"
          >
            <svg class="w-5 h-5 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- 大小调节 -->
        <div class="mb-6">
          <div class="flex items-center justify-between mb-2">
            <span class="text-sm font-medium text-[var(--mobile-text-secondary)]">{{ t('mobile.inputAssistant.ballSize') }}</span>
            <span class="text-sm text-[var(--mobile-accent)]">{{ localSettings.size }}px</span>
          </div>
          <input
            type="range"
            v-model.number="localSettings.size"
            min="36"
            max="64"
            step="4"
            class="w-full h-2 bg-[var(--mobile-accent-muted)] rounded-lg appearance-none cursor-pointer accent-[var(--mobile-accent)]"
          />
          <div class="flex justify-between text-xs text-[var(--mobile-text-muted)] mt-1">
            <span>36px</span>
            <span>64px</span>
          </div>
        </div>

        <!-- 手势开关 -->
        <div class="mb-6">
          <span class="text-sm font-medium text-[var(--mobile-text-secondary)] mb-3 block">{{ t('mobile.inputAssistant.gestures') }}</span>

          <div class="space-y-3">
            <div class="flex items-center justify-between">
              <span class="text-sm text-[var(--mobile-text-secondary)]">{{ t('mobile.inputAssistant.doubleTap') }}</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.doubleTap ? 'bg-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)]'"
                @click="localSettings.gestures.doubleTap = !localSettings.gestures.doubleTap"
              >
                <span
                  class="block w-5 h-5 bg-[var(--mobile-text-primary)] rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.doubleTap ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-[var(--mobile-text-secondary)]">{{ t('mobile.inputAssistant.swipeDown') }}</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeDown ? 'bg-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)]'"
                @click="localSettings.gestures.swipeDown = !localSettings.gestures.swipeDown"
              >
                <span
                  class="block w-5 h-5 bg-[var(--mobile-text-primary)] rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeDown ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-[var(--mobile-text-secondary)]">{{ t('mobile.inputAssistant.swipeUp') }}</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeUp ? 'bg-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)]'"
                @click="localSettings.gestures.swipeUp = !localSettings.gestures.swipeUp"
              >
                <span
                  class="block w-5 h-5 bg-[var(--mobile-text-primary)] rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeUp ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-[var(--mobile-text-secondary)]">{{ t('mobile.inputAssistant.swipeLeft') }}</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeLeft ? 'bg-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)]'"
                @click="localSettings.gestures.swipeLeft = !localSettings.gestures.swipeLeft"
              >
                <span
                  class="block w-5 h-5 bg-[var(--mobile-text-primary)] rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeLeft ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-[var(--mobile-text-secondary)]">{{ t('mobile.inputAssistant.swipeRight') }}</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeRight ? 'bg-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)]'"
                @click="localSettings.gestures.swipeRight = !localSettings.gestures.swipeRight"
              >
                <span
                  class="block w-5 h-5 bg-[var(--mobile-text-primary)] rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeRight ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>
          </div>
        </div>

        <!-- 恢复默认按钮 -->
        <button
          class="w-full py-2.5 text-sm text-[var(--mobile-text-muted)] border border-[var(--mobile-border)] rounded-lg hover:bg-[var(--mobile-accent-muted)] transition-colors"
          @click="handleReset"
        >
          {{ t('mobile.inputAssistant.resetDefaults') }}
        </button>

        <!-- 保存按钮 -->
        <button
          class="w-full mt-3 py-2.5 text-sm font-medium text-[var(--mobile-accent)] bg-[var(--mobile-accent-muted)] border border-[var(--mobile-accent)] rounded-lg hover:opacity-80 transition-colors"
          @click="handleSave"
        >
          {{ t('mobile.inputAssistant.saveSettings') }}
        </button>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useInputAssistantStore } from '@/modules/shared/stores/inputAssistant'

const { t } = useI18n()

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const store = useInputAssistantStore()

// 本地设置副本
const localSettings = ref({
  size: store.settings.size,
  gestures: { ...store.settings.gestures }
})

// 监听弹窗打开，同步设置
watch(() => props.visible, (show) => {
  if (show) {
    localSettings.value = {
      size: store.settings.size,
      gestures: { ...store.settings.gestures }
    }
  }
})

function handleSave() {
  store.saveSettings(localSettings.value)
  emit('close')
}

function handleReset() {
  store.resetSettings()
  localSettings.value = {
    size: 48,
    gestures: {
      doubleTap: true,
      swipeDown: true,
      swipeUp: true,
      swipeLeft: true,
      swipeRight: true,
    }
  }
}
</script>