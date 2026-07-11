<template>
  <div class="flex gap-2 items-end">
    <textarea
      ref="inputRef"
      v-model="text"
      :placeholder="placeholder"
      :disabled="disabled"
      rows="1"
      class="flex-1 resize-none bg-[var(--bg-card)] border border-[var(--border)] rounded-input px-3 py-2 text-sm text-[var(--text-primary)] placeholder-[var(--text-tertiary)] focus:border-brand outline-none"
      @keydown.enter.exact.prevent="handleSend"
      @input="autoResize"
    ></textarea>
    <button
      :disabled="disabled || !text.trim()"
      class="px-3 py-2 bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-btn text-sm font-medium transition-colors flex-shrink-0"
      @click="handleSend"
    >
      {{ label }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = withDefaults(defineProps<{
  disabled?: boolean
  placeholder?: string
}>(), {
  disabled: false,
  placeholder: '',
})

const label = t('desktop.plugin.aiChatbox.send')

const emit = defineEmits<{
  send: [content: string]
}>()

const text = ref('')
const inputRef = ref<HTMLTextAreaElement | null>(null)

function handleSend(): void {
  const content = text.value.trim()
  if (!content || props.disabled) return
  emit('send', content)
  text.value = ''
  nextTick(() => autoResize())
}

function autoResize(): void {
  const el = inputRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 120) + 'px'
}
</script>
