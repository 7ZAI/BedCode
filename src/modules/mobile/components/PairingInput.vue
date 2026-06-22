<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4 overflow-y-auto mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]"></div>

        <!-- Panel - 居中显示，使用自带数字键盘 -->
        <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 my-4">
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
          <h3 class="text-xl font-semibold text-[var(--mobile-text-primary)] text-center mt-2 mb-2">{{ t('mobile.pairing.title') }}</h3>
          <p class="text-[var(--mobile-text-muted)] text-center text-sm mb-6">
            {{ t('mobile.pairing.hint') }}
          </p>

          <!-- Code input display -->
          <div class="flex justify-center gap-2 mb-6">
            <div
              v-for="i in 6"
              :key="i"
              class="w-12 h-14 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-lg flex items-center justify-center text-2xl font-bold"
              :class="code[i-1] ? 'text-[var(--mobile-accent)] border-[var(--mobile-accent)] shadow-[0_0_10px_var(--mobile-accent-muted)]' : 'text-[var(--mobile-text-disabled)]'"
            >
              {{ code[i-1] || '-' }}
            </div>
          </div>

          <!-- Numeric keypad - 自带键盘，不会被输入法遮挡 -->
          <div class="grid grid-cols-3 gap-3 mb-4">
            <button
              v-for="n in 9"
              :key="n"
              class="h-14 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-xl text-xl font-medium text-[var(--mobile-text-primary)] hover:border-[var(--mobile-accent)] transition-colors"
              @click="pressKey(n.toString())"
            >
              {{ n }}
            </button>
            <button
              class="h-14 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-xl text-sm text-[var(--mobile-text-secondary)] hover:border-[var(--mobile-accent)] transition-colors"
              @click="clearCode"
            >
              {{ t('mobile.pairing.clear') }}
            </button>
            <button
              class="h-14 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-xl text-xl font-medium text-[var(--mobile-text-primary)] hover:border-[var(--mobile-accent)] transition-colors"
              @click="pressKey('0')"
            >
              0
            </button>
            <button
              class="h-14 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-xl hover:border-[var(--mobile-accent)] transition-colors"
              @click="backspace"
            >
              <svg class="w-6 h-6 mx-auto text-[var(--mobile-text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2M3 12l6.414 6.414a2 2 0 001.414.586H19a2 2 0 002-2V7a2 2 0 00-2-2h-8.172a2 2 0 00-1.414.586L3 12z" />
              </svg>
            </button>
          </div>

          <!-- Error message -->
          <p v-if="error" class="text-[var(--mobile-error)] text-center text-sm mb-4">
            {{ error }}
          </p>

          <!-- Submit button -->
          <button
            class="w-full bg-[var(--mobile-accent-muted)] border border-[var(--mobile-accent)] text-[var(--mobile-accent)] py-3 rounded-xl font-medium hover:opacity-80 transition-colors"
            :class="{ 'opacity-50': code.length !== 6 || loading }"
            :disabled="code.length !== 6 || loading"
            @click="submit"
          >
            {{ loading ? t('mobile.pairing.verifying') : t('mobile.pairing.confirm') }}
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
  loading?: boolean
  error?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [code: string]
}>()

const code = ref('')

// 弹窗关闭时重置
watch(() => props.modelValue, (value) => {
  if (!value) {
    code.value = ''
  }
})

function pressKey(key: string) {
  if (code.value.length < 6) {
    code.value += key
  }
}

function backspace() {
  code.value = code.value.slice(0, -1)
}

function clearCode() {
  code.value = ''
}

function close() {
  code.value = ''
  emit('update:modelValue', false)
}

function submit() {
  if (code.value.length === 6) {
    emit('submit', code.value)
  }
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
