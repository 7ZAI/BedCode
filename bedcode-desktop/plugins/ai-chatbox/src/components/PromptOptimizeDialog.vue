<template>
  <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center p-4">
    <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" @click="emit('cancel')"></div>
    <div class="relative bg-[var(--bg-card)] rounded-xl shadow-2xl border border-[var(--border)] w-full max-w-lg">
      <div class="px-5 py-3 border-b border-[var(--border)]">
        <h3 class="text-base font-semibold text-[var(--text-primary)]">{{ t('desktop.plugin.aiChatbox.optimizeTitle') }}</h3>
      </div>
      <div class="p-5 space-y-4">
        <div v-if="error" class="p-3 bg-[var(--color-danger-light)] text-[var(--color-danger)] text-sm rounded-lg">
          {{ error }}
        </div>
        <div v-else-if="optimizing" class="flex items-center justify-center py-8">
          <div class="flex items-center gap-2 text-[var(--text-secondary)]">
            <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.969 7.969 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            {{ t('desktop.plugin.aiChatbox.optimizing') }}
          </div>
        </div>
        <template v-else>
          <div>
            <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1">{{ t('desktop.plugin.aiChatbox.originalLabel') }}</label>
            <div class="p-3 bg-[var(--bg-hover)] rounded-lg text-sm text-[var(--text-secondary)] whitespace-pre-wrap">{{ original }}</div>
          </div>
          <div>
            <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1">{{ t('desktop.plugin.aiChatbox.optimizedLabel') }}</label>
            <div class="p-3 bg-brand-light rounded-lg text-sm text-[var(--text-brand)] whitespace-pre-wrap border border-brand/20">{{ optimized }}</div>
          </div>
        </template>
      </div>
      <div v-if="!optimizing && !error" class="px-5 py-3 border-t border-[var(--border)] flex justify-end gap-2">
        <button class="px-4 py-2 text-sm bg-[var(--bg-hover)] text-[var(--text-primary)] rounded-btn hover:bg-[var(--bg-hover)]/80 transition-colors" @click="emit('cancel')">{{ t('desktop.plugin.aiChatbox.cancel') }}</button>
        <button :disabled="!optimized" class="px-4 py-2 text-sm bg-brand hover:bg-brand-hover disabled:opacity-50 text-white rounded-btn transition-colors" @click="emit('accept')">{{ t('desktop.plugin.aiChatbox.acceptAndFill') }}</button>
      </div>
      <div v-else-if="error" class="px-5 py-3 border-t border-[var(--border)] flex justify-end">
        <button class="px-4 py-2 text-sm bg-[var(--bg-hover)] text-[var(--text-primary)] rounded-btn hover:bg-[var(--bg-hover)]/80 transition-colors" @click="emit('cancel')">{{ t('desktop.plugin.aiChatbox.close') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

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
