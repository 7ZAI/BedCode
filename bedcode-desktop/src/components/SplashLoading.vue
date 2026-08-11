<template>
  <Teleport to="body">
    <Transition name="splash">
      <div
        v-if="visible"
        class="fixed inset-0 z-50 flex flex-col items-center justify-center bg-dark-900"
      >
        <!-- Logo -->
        <div class="mb-8">
          <slot name="logo">
            <!-- 默认 Logo：文字 + 图标 -->
            <div class="flex items-center gap-3">
              <div class="w-12 h-12 rounded-xl bg-primary-600 flex items-center justify-center">
                <svg class="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                </svg>
              </div>
              <span class="text-2xl font-semibold text-white">BedCode</span>
            </div>
          </slot>
        </div>

        <!-- Spinner -->
        <Spinner size="lg" color="primary" variant="circle" class="mb-6" />

        <!-- Status Text -->
        <p class="text-dark-300 text-sm mb-2">{{ status }}</p>

        <!-- Progress Bar (可选) -->
        <div v-if="showProgress" class="w-48 h-1 bg-dark-700 rounded-full overflow-hidden">
          <div
            class="h-full bg-primary-500 transition-all duration-300"
            :style="{ width: `${progress}%` }"
          ></div>
        </div>

        <!-- Progress Percentage (可选) -->
        <p v-if="showProgress" class="text-dark-400 text-xs mt-2">{{ progress }}%</p>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import Spinner from './Spinner.vue'

interface Props {
  visible: boolean
  status?: string
  showProgress?: boolean
  progress?: number
}

withDefaults(defineProps<Props>(), {
  status: 'Loading...',
  showProgress: false,
  progress: 0,
})
</script>

<style scoped>
.splash-enter-active,
.splash-leave-active {
  transition: opacity 0.3s ease;
}

.splash-enter-from,
.splash-leave-to {
  opacity: 0;
}
</style>