<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/60" @click="close"></div>

        <!-- Panel - 居中显示，避免被输入法遮挡 -->
        <div class="relative w-full max-w-sm bg-dark-800 rounded-2xl p-6">
          <!-- Close button -->
          <button
            class="absolute top-4 right-4 p-2 text-dark-400 hover:text-white"
            @click="close"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>

          <!-- Title -->
          <h3 class="text-lg font-semibold mt-2 mb-6">{{ title }}</h3>

          <!-- Input field -->
          <div class="mb-4">
            <input
              ref="inputRef"
              v-model="inputValue"
              type="text"
              :placeholder="placeholder"
              class="w-full bg-dark-700 border border-dark-600 rounded-xl px-4 py-3 text-white placeholder-dark-400 focus:outline-none focus:border-primary-500"
              @keyup.enter="submit"
            />
          </div>

          <!-- Actions -->
          <div class="flex gap-3">
            <button
              class="flex-1 bg-dark-700 text-dark-300 py-3 rounded-xl font-medium active:bg-dark-600"
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
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [value: string]
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
  emit('update:modelValue', false)
}

function submit() {
  if (inputValue.value.trim()) {
    emit('submit', inputValue.value.trim())
    close()
  }
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
