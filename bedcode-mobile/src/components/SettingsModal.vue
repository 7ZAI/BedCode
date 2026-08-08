<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div
        v-if="visible"
        class="settings-modal-overlay mobile-ui"
        @click.self="emit('close')"
      >
        <div class="settings-modal modal-panel">
          <!-- Header -->
          <div class="settings-header">
            <h2>{{ t('mobile.inputAssistant.title') }}</h2>
            <button class="close-btn" @click.stop="emit('close')">
              <svg width="24" height="24" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div class="settings-content">
            <!-- 悬浮球大小 -->
            <div class="settings-section">
              <label class="settings-label">{{ t('mobile.inputAssistant.ballSize') }}</label>
              <div class="stepper-control">
                <button
                  class="stepper-btn"
                  @click.stop="localSettings.size--"
                  :disabled="localSettings.size <= 36"
                >-</button>
                <span class="stepper-value">{{ localSettings.size }}px</span>
                <button
                  class="stepper-btn"
                  @click.stop="localSettings.size++"
                  :disabled="localSettings.size >= 64"
                >+</button>
              </div>
            </div>

            <!-- 手势开关 -->
            <div class="settings-section">
              <label class="settings-label">{{ t('mobile.inputAssistant.gestures') }}</label>
              <div class="toggle-list">
                <div
                  v-for="item in gestureItems"
                  :key="item.key"
                  class="toggle-row"
                >
                  <span class="toggle-label">{{ item.label }}</span>
                  <button
                    class="toggle-btn"
                    :class="{ active: localSettings.gestures[item.key] }"
                    @click.stop="localSettings.gestures[item.key] = !localSettings.gestures[item.key]"
                  >
                    <span
                      class="toggle-thumb"
                      :class="{ on: localSettings.gestures[item.key] }"
                    ></span>
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- Footer -->
          <div class="settings-footer">
            <button class="settings-footer-btn cancel" @click.stop="handleReset">
              {{ t('common.button.reset') }}
            </button>
            <button class="settings-footer-btn confirm" @click.stop="handleSave">
              {{ t('common.button.save') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 输入助手设置弹窗 - 悬浮球大小、手势开关
 *
 * 所有编辑中的状态在组件内部管理，确认时通过 store 保存
 */
import { ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useInputAssistantStore } from '@/stores/inputAssistant'

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

const gestureItems = computed(() => [
  { key: 'doubleTap' as const, label: t('mobile.inputAssistant.doubleTap') },
  { key: 'swipeDown' as const, label: t('mobile.inputAssistant.swipeDown') },
  { key: 'swipeUp' as const, label: t('mobile.inputAssistant.swipeUp') },
  { key: 'swipeLeft' as const, label: t('mobile.inputAssistant.swipeLeft') },
  { key: 'swipeRight' as const, label: t('mobile.inputAssistant.swipeRight') },
])

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

<style scoped>
.settings-modal-overlay {
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

.settings-modal {
  --stepper-btn: clamp(2rem, 2.5rem, 3rem);
  --footer-btn-py: clamp(0.625rem, 0.75rem, 1rem);
  --thumb-size: clamp(1rem, 1.375rem, 1.75rem);
  --toggle-w: clamp(2.25rem, 2.75rem, 3.25rem);
  --toggle-h: clamp(1.25rem, 1.5rem, 1.75rem);

  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 100%;
  max-width: clamp(280px, 360px, 420px);
  max-height: 80vh;
  overflow-y: auto;
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--mobile-border);
}

.settings-header h2 {
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
}

.close-btn {
  padding: 0.25rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease;
}

.close-btn:hover {
  color: var(--mobile-text-primary);
}

.settings-content {
  padding: 1rem;
}

.settings-section {
  margin-bottom: 1.5rem;
}

.settings-section:last-child {
  margin-bottom: 0;
}

.settings-label {
  display: block;
  font-size: var(--font-size-base);
  font-weight: 500;
  color: var(--mobile-text-muted);
  margin-bottom: 0.75rem;
}

/* Stepper control */
.stepper-control {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.stepper-btn {
  width: var(--stepper-btn);
  height: var(--stepper-btn);
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-primary);
  font-size: clamp(1rem, 1.25rem, 1.5rem);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.stepper-btn:hover:not(:disabled) {
  background: var(--mobile-bg-hover);
}

.stepper-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.stepper-value {
  flex: 1;
  text-align: center;
  font-size: clamp(1rem, 1.125rem, 1.25rem);
  font-weight: 500;
  color: var(--mobile-text-primary);
}

/* Toggle list */
.toggle-list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.toggle-label {
  font-size: var(--font-size-base);
  color: var(--mobile-text-secondary);
}

.toggle-btn {
  width: var(--toggle-w);
  height: var(--toggle-h);
  border-radius: calc(var(--toggle-h) / 2);
  background: var(--mobile-bg-elevated);
  border: none;
  cursor: pointer;
  position: relative;
  transition: background 0.2s ease;
  flex-shrink: 0;
}

.toggle-btn.active {
  background: var(--mobile-accent);
}

.toggle-thumb {
  position: absolute;
  top: 0.125rem;
  left: 0.125rem;
  width: calc(var(--toggle-h) - 0.25rem);
  height: calc(var(--toggle-h) - 0.25rem);
  border-radius: 50%;
  background: var(--mobile-text-primary);
  box-shadow: 0 1px 3px var(--mobile-overlay-light);
  transition: transform 0.2s ease;
}

.toggle-thumb.on {
  transform: translateX(calc(var(--toggle-w) - var(--toggle-h)));
}

/* Footer */
.settings-footer {
  display: flex;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--mobile-border);
}

.settings-footer-btn {
  flex: 1;
  padding: var(--footer-btn-py);
  border-radius: 0.5rem;
  font-size: clamp(0.8125rem, 0.875rem, 1rem);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-footer-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.settings-footer-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.settings-footer-btn.confirm {
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
}

.settings-footer-btn.confirm:hover {
  opacity: 0.9;
}
</style>
