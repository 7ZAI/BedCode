<template>
  <div class="flex items-end gap-2">
    <textarea
      ref="textareaRef"
      v-model="draft"
      rows="1"
      class="flex-1 resize-none min-h-[36px] max-h-40 px-3 py-2 text-sm bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-input)] rounded-input placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-brand focus:shadow-input-focus transition-colors"
      :placeholder="placeholder"
      :disabled="disabled"
      @keydown.enter.exact.prevent="onEnter"
      @keydown.enter.shift.prevent="insertNewline"
    ></textarea>

    <button
      v-if="streaming"
      class="h-9 px-4 flex-shrink-0 rounded-btn text-sm font-medium bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors"
      :title="t('desktop.plugin.aiChatbox.stop')"
      @click="$emit('stop')"
    >
      {{ t('desktop.plugin.aiChatbox.stop') }}
    </button>
    <button
      v-else
      class="h-9 px-4 flex-shrink-0 rounded-btn text-sm font-medium bg-brand text-[var(--color-primary-contrast)] hover:bg-brand-hover transition-colors disabled:opacity-50 disabled:pointer-events-none"
      :disabled="disabled || !draft.trim()"
      @click="send"
    >
      {{ t('desktop.plugin.aiChatbox.send') }}
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * ChatInput — 多行输入框
 *
 * Enter 发送 / Shift+Enter 换行；自适应高度（1~8 行）；
 * 流式期间切换为"停止"按钮。
 */
import { ref, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  disabled?: boolean
  streaming?: boolean
  placeholder?: string
}>()

const emit = defineEmits<{
  send: [content: string]
  stop: []
}>()

const { t } = useI18n()

const draft = ref('')
const textareaRef = ref<HTMLTextAreaElement | null>(null)

/** 自适应高度：内容变化后按 scrollHeight 调整（上限 10rem = max-h-40） */
watch(draft, async () => {
  await nextTick()
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 160) + 'px'
})

function send(): void {
  const content = draft.value.trim()
  if (!content || props.disabled) return
  draft.value = ''
  nextTick(() => {
    const el = textareaRef.value
    if (el) el.style.height = 'auto'
  })
  emit('send', content)
}

function onEnter(): void {
  send()
}

function insertNewline(): void {
  const el = textareaRef.value
  if (!el) return
  const start = el.selectionStart
  draft.value = draft.value.slice(0, start) + '\n' + draft.value.slice(el.selectionEnd)
  nextTick(() => {
    el.selectionStart = el.selectionEnd = start + 1
  })
}

function focusInput(): void {
  textareaRef.value?.focus()
}

defineExpose({ focusInput })
</script>
