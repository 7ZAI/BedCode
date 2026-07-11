<template>
  <div class="space-y-2">
    <div class="flex items-center justify-between">
      <label class="text-sm text-[var(--text-secondary)]">{{ t('desktop.plugin.aiChatbox.modelList') }}</label>
      <button
        class="flex items-center gap-1 px-2 py-1 text-xs rounded-md bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]/80 transition-colors"
        @click="addModel"
      >
        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
        {{ t('desktop.plugin.aiChatbox.addModel') }}
      </button>
    </div>

    <div v-if="models.length === 0" class="text-xs text-[var(--text-tertiary)] py-2">
      {{ t('desktop.plugin.aiChatbox.noModels') }}
    </div>

    <div v-for="(model, index) in models" :key="index" class="flex items-center gap-2">
      <input
        :value="model"
        type="text"
        :placeholder="t('desktop.plugin.aiChatbox.modelId')"
        class="flex-1 bg-[var(--bg-card)] border border-[var(--border)] rounded-md px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors"
        @input="updateModel(index, ($event.target as HTMLInputElement).value)"
      />
      <button
        class="p-1 text-[var(--text-tertiary)] hover:text-[var(--color-danger)] transition-colors"
        @click="removeModel(index)"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 模型列表编辑器 — 管理模型 ID 的增删
 */
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  models: string[]
}>()

const emit = defineEmits<{
  update: [models: string[]]
}>()

function addModel(): void {
  emit('update', [...props.models, ''])
}

function removeModel(index: number): void {
  const updated = [...props.models]
  updated.splice(index, 1)
  emit('update', updated)
}

function updateModel(index: number, value: string): void {
  const updated = [...props.models]
  updated[index] = value
  emit('update', updated)
}
</script>
