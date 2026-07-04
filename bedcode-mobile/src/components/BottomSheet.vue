<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="handleBackdropClick"></div>

        <!-- Panel - 居中显示，避免被输入法遮挡 -->
        <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl modal-panel">
          <!-- Close button (loading时禁用) -->
          <button
            class="absolute top-4 right-4 p-2 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-primary)]"
            :class="{ 'opacity-50 pointer-events-none': loading }"
            :disabled="loading"
            @click="close"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>

          <!-- Title -->
          <h3 class="text-lg font-semibold mt-2 mb-6 text-[var(--mobile-text-primary)]">{{ title }}</h3>

          <!-- Loading state: show spinner and cancel button -->
          <div v-if="loading" class="mb-4">
            <div class="flex items-center justify-center gap-3 py-4">
              <div class="w-6 h-6 border-2 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin"></div>
              <span class="text-[var(--mobile-text-secondary)]">{{ t('mobile.bottomSheet.connecting') }}</span>
            </div>
            <button
              class="w-full bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] py-3 rounded-xl font-medium"
              @click="handleCancel"
            >
              {{ t('mobile.bottomSheet.cancelConnect') }}
            </button>
          </div>

          <!-- Input field (hidden when loading) -->
          <div v-else class="mb-4">
            <input
              ref="inputRef"
              v-model="inputValue"
              type="text"
              :placeholder="placeholder"
              class="w-full bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-xl px-4 py-3 text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-muted)] focus:outline-none focus:border-[var(--mobile-input-focus)]"
              @keyup.enter="submit"
            />
          </div>

          <!-- Actions (hidden when loading) -->
          <div v-if="!loading" class="flex gap-3">
            <button
              class="flex-1 bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] py-3 rounded-xl font-medium active:opacity-80"
              @click="close"
            >
              {{ t('common.button.cancel') }}
            </button>
            <button
              class="flex-1 bg-[var(--mobile-accent)] text-white py-3 rounded-xl font-medium active:opacity-80"
              :class="{ 'opacity-50': !inputValue }"
              :disabled="!inputValue"
              @click="submit"
            >
              {{ t('common.button.confirm') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  modelValue: boolean
  title?: string
  placeholder?: string
  initialValue?: string
  loading?: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [value: string]
  cancel: []
}>()

const inputValue = ref('')
const inputRef = ref<HTMLInputElement | null>(null)

watch(() => props.modelValue, async (value) => {
  if (value) {
    inputValue.value = props.initialValue || ''
    await nextTick()
    inputRef.value?.focus()
  }
})

function close() {
  if (!props.loading) {
    emit('update:modelValue', false)
  }
}

function submit() {
  if (inputValue.value.trim() && !props.loading) {
    emit('submit', inputValue.value.trim())
    // 不在这里关闭，由父组件控制
  }
}

function handleBackdropClick() {
  if (!props.loading) {
    close()
  }
}

function handleCancel() {
  emit('cancel')
  emit('update:modelValue', false)
}
</script>
