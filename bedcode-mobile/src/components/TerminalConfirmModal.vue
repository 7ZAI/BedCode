<template>
  <Teleport to="body">
    <Transition name="center-modal">
    <div v-if="visible" class="confirm-modal-overlay mobile-ui" @click.self="$emit('cancel')">
      <div class="confirm-modal modal-panel" :style="safeAreaStyle">
      <p class="confirm-text">{{ message }}</p>
      <div class="confirm-buttons">
        <button class="confirm-btn cancel" @click.stop="$emit('cancel')">{{ t('common.button.cancel') }}</button>
        <button class="confirm-btn confirm" @click.stop="$emit('confirm')">{{ t('common.button.confirm') }}</button>
      </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 终端确认弹窗 - 通用确认对话框
 */
defineOptions({ name: 'TerminalConfirmModal' })

import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  visible: boolean
  message: string
  safeAreaStyle: Record<string, string>
}>()

defineEmits<{
  confirm: []
  cancel: []
}>()
</script>

<style scoped>
.confirm-modal-overlay {
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

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: clamp(240px, 300px, 360px);
  text-align: center;
}

.confirm-text {
  font-size: clamp(0.8125rem, 1rem + (100vw - 360px) / 840 * 2, 1.125rem);
  color: var(--mobile-text-primary);
  margin: 0 0 1.25rem;
}

.confirm-buttons {
  display: flex;
  gap: 0.75rem;
}

.confirm-btn {
  flex: 1;
  padding: clamp(0.625rem, 0.75rem, 1rem);
  border-radius: 0.5rem;
  font-size: clamp(0.75rem, 0.875rem + (100vw - 360px) / 840 * 2, 1rem);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.confirm-btn.confirm {
  background: #ef4444;
  border: none;
  color: #ffffff;
}

.confirm-btn.confirm:hover {
  background: #dc2626;
}
</style>
