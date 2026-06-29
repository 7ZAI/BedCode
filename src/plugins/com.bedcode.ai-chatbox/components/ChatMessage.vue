<template>
  <div :class="['flex gap-3', message.role === 'user' ? 'justify-end' : 'justify-start']">
    <div
      v-if="message.role === 'assistant'"
      class="w-7 h-7 rounded-full bg-primary-100 dark:bg-primary-900 flex items-center justify-center text-xs flex-shrink-0 mt-1"
    >
      AI
    </div>
    <div
      :class="[
        'max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed',
        message.role === 'user'
          ? 'bg-primary-600 text-white'
          : 'bg-slate-100 dark:bg-dark-700 text-slate-800 dark:text-dark-200'
      ]"
    >
      <div v-if="message.role === 'assistant' && !message.content && streaming" class="flex items-center gap-1">
        <span class="inline-block w-1.5 h-4 bg-primary-500 animate-pulse"></span>
      </div>
      <div v-else-if="message.role === 'assistant'" v-html="renderedContent"></div>
      <div v-else class="whitespace-pre-wrap">{{ message.content }}</div>
    </div>
    <div
      v-if="message.role === 'user'"
      class="w-7 h-7 rounded-full bg-slate-200 dark:bg-dark-600 flex items-center justify-center text-xs flex-shrink-0 mt-1"
    >
      我
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ChatMessage } from '../types'

const props = defineProps<{
  message: ChatMessage
  streaming?: boolean
}>()

const renderedContent = computed(() => {
  let text = props.message.content
  text = text.replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="bg-slate-800 text-green-300 rounded p-2 my-1 overflow-x-auto text-xs"><code>$2</code></pre>')
  text = text.replace(/`([^`]+)`/g, '<code class="bg-slate-200 dark:bg-dark-600 px-1 rounded text-xs">$1</code>')
  text = text.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  text = text.replace(/\*([^*]+)\*/g, '<em>$1</em>')
  text = text.replace(/\n/g, '<br>')
  return text
})
</script>
