<template>
  <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center p-4">
    <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" @click="emit('cancel')"></div>
    <div class="relative bg-white dark:bg-dark-800 rounded-xl shadow-2xl border border-slate-200 dark:border-dark-700 w-full max-w-lg">
      <div class="px-5 py-3 border-b border-slate-100 dark:border-dark-700">
        <h3 class="text-base font-semibold text-slate-800 dark:text-white">AI 提示词优化</h3>
      </div>
      <div class="p-5 space-y-4">
        <div v-if="error" class="p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm rounded-lg">
          {{ error }}
        </div>
        <div v-else-if="optimizing" class="flex items-center justify-center py-8">
          <div class="flex items-center gap-2 text-slate-500 dark:text-dark-400">
            <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.969 7.969 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            AI 正在优化提示词...
          </div>
        </div>
        <template v-else>
          <div>
            <label class="block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">原始提示词</label>
            <div class="p-3 bg-slate-50 dark:bg-dark-700 rounded-lg text-sm text-slate-600 dark:text-dark-300 whitespace-pre-wrap">{{ original }}</div>
          </div>
          <div>
            <label class="block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">优化后提示词</label>
            <div class="p-3 bg-primary-50 dark:bg-primary-900/20 rounded-lg text-sm text-primary-800 dark:text-primary-200 whitespace-pre-wrap border border-primary-200 dark:border-primary-800">{{ optimized }}</div>
          </div>
        </template>
      </div>
      <div v-if="!optimizing && !error" class="px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end gap-2">
        <button class="px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors" @click="emit('cancel')">取消</button>
        <button :disabled="!optimized" class="px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg transition-colors" @click="emit('accept')">采纳并填入终端</button>
      </div>
      <div v-else-if="error" class="px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end">
        <button class="px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors" @click="emit('cancel')">关闭</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  show: boolean
  optimizing: boolean
  original: string
  optimized: string
  error: string
}>()

const emit = defineEmits<{
  accept: []
  cancel: []
}>()
</script>
