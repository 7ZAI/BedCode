<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/60" @click="handleBackdropClick"></div>

        <!-- Panel - 居中显示，避免被输入法遮挡 -->
        <div class="relative w-full max-w-sm bg-white dark:bg-dark-800 rounded-2xl p-6">
          <!-- Close button (loading时禁用) -->
          <button
            class="absolute top-4 right-4 p-2 text-gray- dark:text-dark-400 hover:text-white"
            :class="{ 'opacity-50 pointer-events-none': loading }"
            :disabled="loading"
            @click="close"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>

          <!-- Title -->
          <h3 class="text-lg font-semibold mt-2 mb-6">{{ title }}</h3>

          <!-- Loading state: show spinner and cancel button -->
          <div v-if="loading" class="mb-4">
            <div class="flex items-center justify-center gap-3 py-4">
              <div class="w-6 h-6 border-2 border-primary-500 border-t-transparent rounded-full animate-spin"></div>
              <span class="text-gray- dark:text-dark-300">正在连接...</span>
            </div>
            <button
              class="w-full bg-gray-100 dark:bg-dark-700 text-gray- dark:text-dark-300 py-3 rounded-xl font-medium"
              @click="handleCancel"
            >
              取消连接
            </button>
          </div>

          <!-- Input field (hidden when loading) -->
          <div v-else class="mb-4">
            <input
              ref="inputRef"
              v-model="inputValue"
              type="text"
              :placeholder="placeholder"
              class="w-full bg-gray-100 dark:bg-dark-700 border border-gray-300 dark:border-dark-600 rounded-xl px-4 py-3 text-white placeholder-dark-400 focus:outline-none focus:border-primary-500"
              @keyup.enter="submit"
            />
          </div>

          <!-- Actions (hidden when loading) -->
          <div v-if="!loading" class="flex gap-3">
            <button
              class="flex-1 bg-gray-100 dark:bg-dark-700 text-gray- dark:text-dark-300 py-3 rounded-xl font-medium active:bg-gray-200 dark:bg-dark-600"
              @click="close"
            >
              取消
            </button>
            <button
              class="flex-1 bg-primary-600 text-white py-3 rounded-xl font-medium active:bg-primary-500"
              :class="{ 'opacity-50': !inputValue }"
              :disabled="!inputValue"
              @click="submit"
            >
              确定
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'

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

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-active .relative,
.fade-leave-active .relative {
  transition: transform 0.2s ease, opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.fade-enter-from .relative,
.fade-leave-to .relative {
  transform: scale(0.95);
  opacity: 0;
}
</style>
