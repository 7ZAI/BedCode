<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4 overflow-y-auto mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="close"></div>

        <!-- Panel -->
        <div class="relative w-full max-w-[clamp(280px,384px,440px)] bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 m-4">
          <!-- Close button -->
          <button
            class="absolute top-4 right-4 p-2 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-accent)] transition-colors"
            @click="close"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>

          <!-- Title -->
          <h3 class="text-xl font-semibold text-[var(--mobile-text-primary)] text-center mt-2 mb-2">{{ t('mobile.connection.authTitle') }}</h3>
          <p class="text-[var(--mobile-text-muted)] text-center text-sm mb-5">
            {{ t('mobile.connection.authHint') }}
          </p>

          <!-- 生物认证选项 -->
          <button
            v-if="canBiometric"
            class="w-full flex items-center gap-3 p-4 rounded-xl border mb-3 text-left transition-all duration-200 active:opacity-80"
            :class="selected === 'biometric'
              ? 'bg-[color:color-mix(in_srgb,var(--mobile-accent)_15%,transparent)] border-[var(--mobile-accent)]'
              : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)]'"
            @click="selected = 'biometric'"
          >
            <span class="flex-shrink-0 flex items-center justify-center w-10 h-10 rounded-lg"
              :class="selected === 'biometric' ? 'bg-[color:color-mix(in_srgb,var(--mobile-accent)_20%,transparent)] text-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)]'">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
              </svg>
            </span>
            <span class="min-w-0">
              <span class="block text-sm font-medium text-[var(--mobile-text-primary)]">{{ t('mobile.connection.authBiometric') }}</span>
              <span class="block text-xs text-[var(--mobile-text-muted)] mt-0.5">{{ t('mobile.connection.authBiometricDesc') }}</span>
            </span>
            <span v-if="selected === 'biometric'" class="ml-auto text-[var(--mobile-accent)]">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
              </svg>
            </span>
          </button>

          <!-- 配对码选项 -->
          <button
            class="w-full flex items-center gap-3 p-4 rounded-xl border mb-4 text-left transition-all duration-200 active:opacity-80"
            :class="selected === 'pairing'
              ? 'bg-[color:color-mix(in_srgb,var(--mobile-accent)_15%,transparent)] border-[var(--mobile-accent)]'
              : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border)]'"
            @click="selected = 'pairing'"
          >
            <span class="flex-shrink-0 flex items-center justify-center w-10 h-10 rounded-lg"
              :class="selected === 'pairing' ? 'bg-[color:color-mix(in_srgb,var(--mobile-accent)_20%,transparent)] text-[var(--mobile-accent)]' : 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)]'">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
              </svg>
            </span>
            <span class="min-w-0">
              <span class="block text-sm font-medium text-[var(--mobile-text-primary)]">{{ t('mobile.connection.authPairing') }}</span>
              <span class="block text-xs text-[var(--mobile-text-muted)] mt-0.5">{{ t('mobile.connection.authPairingDesc') }}</span>
            </span>
            <span v-if="selected === 'pairing'" class="ml-auto text-[var(--mobile-accent)]">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
              </svg>
            </span>
          </button>

          <!-- Error message -->
          <p v-if="error" class="text-[var(--mobile-error)] text-center text-sm mb-4">
            {{ error }}
          </p>

          <!-- Confirm button -->
          <button
            class="w-full py-3 rounded-xl text-sm font-medium transition-all duration-200 active:opacity-80"
            :class="loading
              ? 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-muted)]'
              : 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'"
            :disabled="loading"
            @click="confirm"
          >
            <span v-if="loading" class="inline-flex items-center gap-2">
              <svg class="w-4 h-4 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              {{ t('mobile.connection.authProcessing') }}
            </span>
            <span v-else>{{ t('mobile.connection.authConfirm') }}</span>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  modelValue: boolean
  /** 生物认证是否可用（设备支持 + 已绑定） */
  canBiometric?: boolean
  /** 弹窗内错误提示（生物认证失败等） */
  error?: string
  /** 认证执行中（禁用交互） */
  loading?: boolean
  /** 默认优先的认证方式（来自设置） */
  defaultMethod?: 'biometric' | 'pairing'
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  confirm: [method: 'biometric' | 'pairing']
  close: []
}>()

const selected = ref<'biometric' | 'pairing'>('pairing')

// 打开时根据设置确定默认选中：设置的方式可用则用它，否则用另一项
watch(() => props.modelValue, (value) => {
  if (value) {
    if (props.defaultMethod === 'biometric' && props.canBiometric) {
      selected.value = 'biometric'
    } else {
      selected.value = 'pairing'
    }
  }
})

function confirm() {
  if (!props.loading) {
    emit('confirm', selected.value)
  }
}

function close() {
  emit('update:modelValue', false)
  emit('close')
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
