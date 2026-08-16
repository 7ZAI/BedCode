<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4 overflow-y-auto mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]"></div>

        <!-- Panel - 居中显示，使用自带数字键盘 -->
        <div class="pairing-panel">
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
          <div class="code-cells">
            <div
              v-for="i in 6"
              :key="i"
              class="code-cell"
              :class="code[i-1] ? 'code-cell--filled' : 'code-cell--empty'"
            >
              {{ code[i-1] || '-' }}
            </div>
          </div>

          <!-- Numeric keypad - 自带键盘，不会被输入法遮挡 -->
          <div class="keypad">
            <button
              v-for="n in 9"
              :key="n"
              class="key-btn"
              @click="pressKey(n.toString())"
            >
              {{ n }}
            </button>
            <button
              class="key-btn key-btn--small-text"
              @click="clearCode"
            >
              {{ t('mobile.pairing.clear') }}
            </button>
            <button
              class="key-btn"
              @click="pressKey('0')"
            >
              0
            </button>
            <button
              class="key-btn key-btn--icon"
              @click="backspace"
            >
              <svg class="key-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
            class="submit-btn"
            :class="{ 'opacity-50': code.length !== 6 || loading }"
            :disabled="code.length !== 6 || loading"
            @click="submit"
          >
            {{ loading ? t('mobile.pairing.verifying') : t('mobile.pairing.confirm') }}
          </button>

          <!-- 切换认证方式：生物认证是便捷方式，可随时切过去（未绑定时父组件提示） -->
          <div class="text-center mt-4">
            <button
              class="text-sm transition-colors active:opacity-80"
              style="color: var(--mobile-accent)"
              :disabled="loading"
              @click="emit('switch')"
            >
              {{ t('mobile.connection.switchToBiometric') }}
            </button>
          </div>
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
  /** 用户手动关闭弹窗（点击 X） */
  close: []
  /** 请求切换到生物认证 */
  switch: []
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
  emit('close')
}

function submit() {
  if (code.value.length === 6) {
    emit('submit', code.value)
  }
}
</script>

<style scoped>
.pairing-panel {
  --cell-w: clamp(2.5rem, 3rem, 3.5rem);
  --cell-h: clamp(2.75rem, 3.5rem, 4rem);
  --cell-font: clamp(1.25rem, 1.5rem, 1.75rem);
  --key-h: clamp(2.75rem, 3.5rem, 4rem);
  --key-font: clamp(1rem, 1.25rem, 1.5rem);
  --key-gap: clamp(0.5rem, 0.75rem, 1rem);

  position: relative;
  width: 100%;
  max-width: clamp(280px, 384px, 440px);
  background: var(--mobile-bg-card);
  border: 1px solid var(--mobile-border);
  border-radius: 1rem;
  padding: clamp(1rem, 1.5rem, 2rem);
  margin: 1rem 0;
}

.code-cells {
  display: flex;
  justify-content: center;
  gap: var(--key-gap);
  margin-bottom: 1.5rem;
}

.code-cell {
  width: var(--cell-w);
  height: var(--cell-h);
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--cell-font);
  font-weight: 700;
}

.code-cell--filled {
  color: var(--mobile-accent);
  border-color: var(--mobile-accent);
  box-shadow: 0 0 10px var(--mobile-accent-muted);
}

.code-cell--empty {
  color: var(--mobile-text-disabled);
}

.keypad {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--key-gap);
  margin-bottom: 1rem;
}

.key-btn {
  height: var(--key-h);
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  font-size: var(--key-font);
  font-weight: 500;
  color: var(--mobile-text-primary);
  cursor: pointer;
  transition: border-color 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.key-btn:hover {
  border-color: var(--mobile-accent);
}

.key-btn--small-text {
  font-size: clamp(0.6875rem, 0.875rem + (100vw - 360px) / 840 * 2, 1rem);
  color: var(--mobile-text-secondary);
}

.key-btn--icon {
  color: var(--mobile-text-secondary);
}

.key-icon {
  width: clamp(1.25rem, 1.5rem, 1.75rem);
  height: clamp(1.25rem, 1.5rem, 1.75rem);
}

.submit-btn {
  width: 100%;
  background: var(--mobile-accent-muted);
  border: 1px solid var(--mobile-accent);
  color: var(--mobile-accent);
  padding: clamp(0.625rem, 0.75rem, 1rem) 0;
  border-radius: 0.75rem;
  font-weight: 500;
  cursor: pointer;
  transition: opacity 0.2s ease;
}

.submit-btn:hover {
  opacity: 0.8;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
