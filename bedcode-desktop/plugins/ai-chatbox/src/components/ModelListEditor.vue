<template>
  <div class="space-y-2">
    <!-- 模型列表 -->
    <div v-if="models.length > 0" class="space-y-1">
      <div
        v-for="(m, i) in models"
        :key="i"
        class="flex items-center gap-2 px-3 h-[36px] bg-[var(--bg-input)] border border-[var(--border-input)] rounded-input"
      >
        <span class="flex-1 min-w-0 truncate text-sm text-[var(--text-primary)] font-mono">{{ m }}</span>
        <button
          class="p-1 text-[var(--text-tertiary)] hover:text-[var(--color-danger)] rounded transition-colors flex-shrink-0"
          :title="t('desktop.plugin.aiChatbox.removeModel')"
          @click="removeModel(i)"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
    <div v-else class="px-3 py-2 text-xs text-[var(--text-tertiary)]">
      {{ t('desktop.plugin.aiChatbox.noModels') }}
    </div>

    <!-- 添加模型 -->
    <div class="flex gap-2">
      <input
        v-model="draft"
        type="text"
        class="flex-1 h-[36px] px-3 text-sm bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-input)] rounded-input placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-brand transition-colors"
        :placeholder="t('desktop.plugin.aiChatbox.modelId')"
        @keydown.enter="addModel"
      />
      <button
        class="h-[36px] px-3 text-sm rounded-btn bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors flex-shrink-0"
        :disabled="!draft.trim()"
        @click="addModel"
      >
        {{ t('desktop.plugin.aiChatbox.addModel') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ModelListEditor — 模型列表编辑（v-model:models 双向绑定）
 */
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ models: string[] }>()
const emit = defineEmits<{ 'update:models': [models: string[]] }>()

const { t } = useI18n()

const draft = ref('')

function addModel(): void {
  const id = draft.value.trim()
  if (!id || props.models.includes(id)) return
  emit('update:models', [...props.models, id])
  draft.value = ''
}

function removeModel(idx: number): void {
  emit('update:models', props.models.filter((_, i) => i !== idx))
}
</script>
