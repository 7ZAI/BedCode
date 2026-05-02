<template>
  <div class="input-bar bg-dark-800 border-t border-dark-700 p-3">
    <!-- Main input row -->
    <div class="flex items-center gap-2">
      <!-- Input field -->
      <div class="flex-1 relative">
        <input
          ref="inputRef"
          v-model="inputText"
          type="text"
          :placeholder="placeholder"
          :disabled="disabled"
          class="w-full bg-dark-700 border border-dark-600 rounded-xl px-4 py-2.5 pr-10 text-white placeholder-dark-400 focus:outline-none focus:border-primary-500 disabled:opacity-50"
          @keyup.enter="submitText"
          @focus="showSpecialKeys = false"
        />
        <!-- Send button -->
        <button
          v-if="inputText"
          class="absolute right-2 top-1/2 -translate-y-1/2 p-1.5 text-primary-400"
          @click="submitText"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
          </svg>
        </button>
      </div>

      <!-- Special keys toggle -->
      <button
        class="p-2.5 rounded-xl"
        :class="showSpecialKeys ? 'bg-primary-600 text-white' : 'bg-dark-700 text-dark-400'"
        @click="toggleSpecialKeys"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
        </svg>
      </button>
    </div>

    <!-- Special keys panel -->
    <Transition name="slide">
      <div v-if="showSpecialKeys" class="mt-3 grid grid-cols-4 gap-2">
        <button
          v-for="key in specialKeys"
          :key="key.code"
          class="bg-dark-700 text-dark-300 text-sm py-2 rounded-lg active:bg-dark-600"
          @click="sendSpecialKey(key.code)"
        >
          {{ key.label }}
        </button>
      </div>
    </Transition>

    <!-- Connection status -->
    <div v-if="showStatus" class="flex items-center justify-center gap-2 mt-2">
      <div
        :class="[
          'w-2 h-2 rounded-full',
          isConnected ? 'bg-green-500' : 'bg-red-500'
        ]"
      ></div>
      <span class="text-xs text-dark-400">
        {{ isConnected ? '已连接' : '未连接' }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

defineProps<{
  disabled?: boolean
  placeholder?: string
  isConnected?: boolean
  showStatus?: boolean
}>()

const emit = defineEmits<{
  submit: [text: string]
  specialKey: [key: string]
}>()

const inputText = ref('')
const inputRef = ref<HTMLInputElement | null>(null)
const showSpecialKeys = ref(false)

const specialKeys = [
  { label: 'Tab', code: 'tab' },
  { label: 'Esc', code: 'escape' },
  { label: 'Ctrl+C', code: 'ctrl_c' },
  { label: 'Ctrl+D', code: 'ctrl_d' },
  { label: 'Ctrl+Z', code: 'ctrl_z' },
  { label: 'Ctrl+L', code: 'ctrl_l' },
  { label: '↑', code: 'up' },
  { label: '↓', code: 'down' },
  { label: '←', code: 'left' },
  { label: '→', code: 'right' },
  { label: 'Home', code: 'home' },
  { label: 'End', code: 'end' },
]

function submitText() {
  if (inputText.value.trim()) {
    emit('submit', inputText.value)
    inputText.value = ''
  }
}

function sendSpecialKey(code: string) {
  emit('specialKey', code)
}

function toggleSpecialKeys() {
  showSpecialKeys.value = !showSpecialKeys.value
}

function focus() {
  inputRef.value?.focus()
}

defineExpose({ focus })
</script>

<style scoped>
.slide-enter-active,
.slide-leave-active {
  transition: all 0.2s ease;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  transform: translateY(-10px);
}
</style>
