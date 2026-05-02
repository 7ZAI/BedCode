<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center p-4 overflow-y-auto">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/80"></div>

        <!-- Panel - 居中显示，使用自带数字键盘 -->
        <div class="relative w-full max-w-sm bg-dark-800 rounded-2xl p-6 my-4">
          <!-- Close button -->
          <button
            class="absolute top-4 right-4 p-2 text-dark-400 hover:text-white active:text-white"
            @click="close"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>

          <!-- Title -->
          <h3 class="text-xl font-semibold text-center mt-2 mb-2">输入配对码</h3>
          <p class="text-dark-400 text-center text-sm mb-6">
            请在桌面端查看并输入 6 位数字配对码
          </p>

          <!-- Code input display -->
          <div class="flex justify-center gap-2 mb-6">
            <div
              v-for="i in 6"
              :key="i"
              class="w-12 h-14 bg-dark-700 rounded-lg flex items-center justify-center text-2xl font-bold"
              :class="code[i-1] ? 'text-white border-2 border-primary-500' : 'text-dark-500'"
            >
              {{ code[i-1] || '-' }}
            </div>
          </div>

          <!-- Numeric keypad - 自带键盘，不会被输入法遮挡 -->
          <div class="grid grid-cols-3 gap-3 mb-4">
            <button
              v-for="n in 9"
              :key="n"
              class="h-14 bg-dark-700 rounded-xl text-xl font-medium active:bg-dark-600 transition-colors"
              @click="pressKey(n.toString())"
            >
              {{ n }}
            </button>
            <button
              class="h-14 bg-dark-700 rounded-xl text-sm text-dark-400 active:bg-dark-600 transition-colors"
              @click="clearCode"
            >
              清除
            </button>
            <button
              class="h-14 bg-dark-700 rounded-xl text-xl font-medium active:bg-dark-600 transition-colors"
              @click="pressKey('0')"
            >
              0
            </button>
            <button
              class="h-14 bg-dark-700 rounded-xl active:bg-dark-600 transition-colors"
              @click="backspace"
            >
              <svg class="w-6 h-6 mx-auto text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2M3 12l6.414 6.414a2 2 0 001.414.586H19a2 2 0 002-2V7a2 2 0 00-2-2h-8.172a2 2 0 00-1.414.586L3 12z" />
              </svg>
            </button>
          </div>

          <!-- Error message -->
          <p v-if="error" class="text-red-400 text-center text-sm mb-4">
            {{ error }}
          </p>

          <!-- Submit button -->
          <button
            class="w-full bg-primary-600 text-white py-3 rounded-xl font-medium active:bg-primary-500 transition-colors"
            :class="{ 'opacity-50': code.length !== 6 || loading }"
            :disabled="code.length !== 6 || loading"
            @click="submit"
          >
            {{ loading ? '验证中...' : '确认配对' }}
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'

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
