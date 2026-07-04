<template>
  <div class="flex gap-2 items-end">
    <textarea
      ref="inputRef"
      v-model="text"
      :placeholder="placeholder"
      :disabled="disabled"
      rows="1"
      class="flex-1 resize-none bg-slate-50 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white placeholder-slate-400 dark:placeholder-dark-500 focus:border-primary-500 outline-none"
      @keydown.enter.exact.prevent="handleSend"
      @input="autoResize"
    ></textarea>
    <button
      :disabled="disabled || !text.trim()"
      class="px-3 py-2 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-medium transition-colors flex-shrink-0"
      @click="handleSend"
    >
      发送
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'

const props = withDefaults(defineProps<{
  disabled?: boolean
  placeholder?: string
}>(), {
  disabled: false,
  placeholder: '输入消息...',
})

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
