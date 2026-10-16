<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="handleBackdropClick"></div>

        <!-- Panel - 居中显示，避免被输入法遮挡 -->
        <div
          class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl modal-panel"
        >
          <!-- Close button (loading 时禁用) -->
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
              <div
                class="w-6 h-6 border-2 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin"
              ></div>
              <span class="text-[var(--mobile-text-secondary)]">{{ connectingText }}</span>
            </div>
            <button
              class="w-full bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] py-3 rounded-xl font-medium"
              @click="handleCancel"
            >
              {{ cancelConnectText }}
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
              {{ cancelText }}
            </button>
            <button
              class="flex-1 bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] py-3 rounded-xl font-medium active:opacity-80"
              :class="{ 'opacity-50': !inputValue }"
              :disabled="!inputValue"
              @click="submit"
            >
              {{ confirmText }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 手动连接弹层（自旧宿主 `src/components/BottomSheet.vue` 迁入插件）
 *
 * 契约与旧实现一致（modelValue / title / placeholder / initialValue / loading +
 * submit / cancel），差异只有一处：按钮与「连接中」文案改为**props 传入**
 * （旧实现读宿主 `mobile.bottomSheet.*` / `common.button.*`），
 * 让插件侧文案归属本域 i18n。
 */
import { ref, watch, nextTick } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: boolean
    title?: string
    placeholder?: string
    initialValue?: string
    loading?: boolean
    /** 加载态文案（连接中…） */
    connectingText?: string
    /** 加载态取消按钮文案 */
    cancelConnectText?: string
    cancelText?: string
    confirmText?: string
  }>(),
  {
    title: '',
    placeholder: '',
    initialValue: '',
    connectingText: '',
    cancelConnectText: '',
    cancelText: '',
    confirmText: '',
  },
)

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [value: string]
  cancel: []
}>()

const inputValue = ref('')
const inputRef = ref<HTMLInputElement | null>(null)

watch(
  () => props.modelValue,
  async (value) => {
    if (value) {
      inputValue.value = props.initialValue || ''
      await nextTick()
      inputRef.value?.focus()
    }
  },
)

function close() {
  if (!props.loading) {
    emit('update:modelValue', false)
  }
}

function submit() {
  if (inputValue.value.trim() && !props.loading) {
    // 不在这里关闭，由父组件控制
    emit('submit', inputValue.value.trim())
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
