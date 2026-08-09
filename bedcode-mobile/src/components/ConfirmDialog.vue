<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-50 flex items-center justify-center mobile-ui"
      >
        <!-- Backdrop -->
        <div
          class="absolute inset-0 bg-[var(--mobile-overlay)] backdrop-blur-sm"
          @click="handleBackdropClick"
        ></div>

        <!-- Panel -->
        <div class="relative w-full max-w-sm mx-4 mb-[var(--safe-area-bottom,0px)] bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl overflow-hidden shadow-xl modal-panel">
          <!-- Header -->
          <div class="px-6 pt-6 pb-2">
            <div class="flex items-center gap-3 mb-2">
              <!-- Icon -->
              <div
                :class="[
                  'w-10 h-10 rounded-xl flex items-center justify-center flex-shrink-0',
                  variant === 'danger' ? 'bg-[var(--mobile-danger-bg)]' :
                  variant === 'warning' ? 'bg-[var(--mobile-warning-muted)]' :
                  'bg-[var(--mobile-accent-muted)]'
                ]"
              >
                <svg
                  :class="[
                    'w-5 h-5',
                    variant === 'danger' ? 'text-[var(--mobile-danger-color)]' :
                    variant === 'warning' ? 'text-[var(--mobile-warning)]' :
                    'text-[var(--mobile-accent)]'
                  ]"
                  fill="none" stroke="currentColor" viewBox="0 0 24 24"
                >
                  <path v-if="variant === 'danger'" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
                  <path v-else-if="variant === 'warning'" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </div>
              <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)]">{{ title }}</h3>
            </div>
            <p class="text-[var(--mobile-text-secondary)] text-sm leading-relaxed">{{ message }}</p>
          </div>

          <!-- Actions -->
          <div class="flex gap-3 px-6 py-5">
            <button
              class="flex-1 bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] rounded-xl font-medium active:opacity-80 transition-colors duration-200 confirm-btn-height"
              :disabled="loading"
              @click="handleCancel"
            >
              {{ cancelText }}
            </button>
            <button
              :class="[
                'flex-1 rounded-xl font-medium active:opacity-80 transition-colors duration-200 confirm-btn-height',
                loading ? 'opacity-50 pointer-events-none' : '',
                variant === 'danger'
                  ? 'bg-[var(--mobile-danger-solid-bg)] text-[var(--mobile-text-on-accent)]'
                  : 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'
              ]"
              :disabled="loading"
              @click="handleConfirm"
            >
              <span v-if="loading" class="flex items-center justify-center gap-2">
                <span class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
                {{ confirmText }}
              </span>
              <span v-else>{{ confirmText }}</span>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * ConfirmDialog - 移动端确认弹窗
 *
 * 底部弹出式确认对话框，支持 danger/warning/info 变体、loading 状态
 * 使用 Teleport + Transition，遵循 z-50 层级规范
 */
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = withDefaults(defineProps<{
  modelValue: boolean
  title: string
  message: string
  variant?: 'info' | 'warning' | 'danger'
  confirmText?: string
  cancelText?: string
  loading?: boolean
  closeOnBackdrop?: boolean
}>(), {
  variant: 'info',
  confirmText: '',
  cancelText: '',
  loading: false,
  closeOnBackdrop: true,
})

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  confirm: []
  cancel: []
}>()

function handleConfirm() {
  if (!props.loading) emit('confirm')
}

function handleCancel() {
  if (!props.loading) {
    emit('cancel')
    emit('update:modelValue', false)
  }
}

function handleBackdropClick() {
  if (props.closeOnBackdrop && !props.loading) {
    emit('update:modelValue', false)
  }
}
</script>

<style scoped>
.confirm-btn-height {
  height: clamp(2.5rem, 2.75rem, 3rem);
}
</style>
