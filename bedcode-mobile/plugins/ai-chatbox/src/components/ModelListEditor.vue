<template>
  <div class="space-y-2">
    <!-- 模型列表 -->
    <div v-if="models.length > 0" class="space-y-1.5">
      <div
        v-for="(m, i) in models"
        :key="i"
        class="flex items-center gap-2 px-3 min-h-[44px] bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-xl"
      >
        <span class="flex-1 min-w-0 truncate text-[var(--font-size-base)] text-[var(--mobile-text-primary)] font-mono">{{ m }}</span>
        <button
          class="w-10 h-10 flex items-center justify-center text-[var(--mobile-text-muted)] active:opacity-80 rounded-lg transition-opacity flex-shrink-0"
          :title="t('mobile.plugin.aiChatbox.removeModel')"
          @click="removeModel(i)"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
    <div v-else class="px-3 py-2 text-xs text-[var(--mobile-text-muted)]">
      {{ t('mobile.plugin.aiChatbox.noModels') }}
    </div>

    <!-- 添加模型 -->
    <div class="flex gap-2">
      <input
        v-model="draft"
        type="text"
        class="flex-1 min-h-[44px] px-3 text-[var(--font-size-base)] bg-[var(--mobile-input-bg)] text-[var(--mobile-text-primary)] border border-[var(--mobile-input-border)] rounded-xl placeholder:text-[var(--mobile-input-placeholder)] focus:outline-none focus:border-[var(--mobile-input-focus)] transition-colors"
        :placeholder="t('mobile.plugin.aiChatbox.modelId')"
        @keydown.enter="addModel"
      />
      <button
        class="min-h-[44px] px-4 text-[var(--font-size-base)] rounded-xl bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity flex-shrink-0 disabled:opacity-40"
        :disabled="!draft.trim()"
        @click="addModel"
      >
        {{ t('mobile.plugin.aiChatbox.addModel') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ModelListEditor — 模型列表编辑（v-model:models 双向绑定，移动端）
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
