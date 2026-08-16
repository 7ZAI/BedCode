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

          <!-- Fingerprint icon -->
          <div class="flex justify-center mt-2 mb-4">
            <span class="flex items-center justify-center w-16 h-16 rounded-2xl bg-[color:color-mix(in_srgb,var(--mobile-accent)_12%,transparent)] text-[var(--mobile-accent)]">
              <svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 11c0 3.517-1.009 6.799-2.753 9.571m-3.44-2.04l.054-.09A13.916 13.916 0 008 8a4 4 0 118 0c0 1.017-.07 2.019-.203 3m-2.118 6.844A21.88 21.88 0 0015.171 17m3.839 1.132c.645-2.266.99-4.659.99-7.132A8 8 0 008 4.07M3 15.364c.64-1.319 1-2.8 1-4.364 0-1.457.39-2.823 1.07-4" />
              </svg>
            </span>
          </div>

          <!-- Title -->
          <h3 class="text-xl font-semibold text-[var(--mobile-text-primary)] text-center mb-2">{{ t('mobile.connection.authBiometric') }}</h3>
          <p class="text-[var(--mobile-text-muted)] text-center text-sm mb-6">
            {{ t('mobile.connection.authBiometricDesc') }}
          </p>

          <!-- Error message -->
          <p v-if="error" class="text-[var(--mobile-error)] text-center text-sm mb-4">
            {{ error }}
          </p>

          <!-- Verify button -->
          <button
            class="w-full py-3 rounded-xl text-sm font-medium transition-opacity duration-200 active:opacity-80"
            :class="loading
              ? 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-muted)]'
              : 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'"
            :disabled="loading"
            @click="emit('authenticate')"
          >
            <span v-if="loading" class="inline-flex items-center gap-2">
              <svg class="w-4 h-4 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              {{ t('mobile.connection.biometricAuthenticating') }}
            </span>
            <span v-else>{{ t('mobile.connection.biometricVerify') }}</span>
          </button>

          <!-- 切换认证方式：配对码是兜底方式，随时可切回 -->
          <div class="text-center mt-4">
            <button
              class="text-sm transition-colors active:opacity-80"
              style="color: var(--mobile-accent)"
              :disabled="loading"
              @click="emit('switch-to-pairing')"
            >
              {{ t('mobile.connection.switchToPairing') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 生物认证弹窗 - 指纹/人脸验证 + 切换配对码入口
 * 打开时由父组件自动触发系统生物识别；失败/取消后弹窗内展示错误，
 * 可重试指纹验证或通过小字链接切换到配对码认证。
 */
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  modelValue: boolean
  /** 弹窗内错误提示（生物认证失败等） */
  error?: string
  /** 认证执行中（禁用交互） */
  loading?: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  authenticate: []
  'switch-to-pairing': []
  close: []
}>()

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
