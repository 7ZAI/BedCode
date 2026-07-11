<template>
  <div :class="['flex gap-3', message.role === 'user' ? 'justify-end' : 'justify-start']">
    <div
      v-if="message.role === 'assistant'"
      class="w-7 h-7 rounded-full bg-brand-light flex items-center justify-center text-xs flex-shrink-0 mt-1"
    >
      AI
    </div>
    <div
      :class="[
        'max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed',
        message.role === 'user'
          ? 'bg-brand text-white'
          : 'bg-[var(--bg-hover)] text-[var(--text-primary)]'
      ]"
    >
      <div v-if="message.role === 'assistant' && !message.content && streaming" class="flex items-center gap-1">
        <span class="inline-block w-1.5 h-4 bg-brand animate-pulse"></span>
      </div>
      <!-- 安全渲染：先转义 HTML，再应用受控 Markdown 转换 -->
      <div v-else-if="message.role === 'assistant'" v-html="renderedContent"></div>
      <div v-else class="whitespace-pre-wrap">{{ message.content }}</div>
    </div>
    <div
      v-if="message.role === 'user'"
      class="w-7 h-7 rounded-full bg-[var(--bg-hover)] flex items-center justify-center text-xs flex-shrink-0 mt-1"
    >
      {{ $t('desktop.plugin.aiChatbox.send').charAt(0) }}
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

/** 转义 HTML 特殊字符，防止 XSS */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}

/** 将转义后的 Markdown 文本安全地转为 HTML */
function renderMarkdown(escaped: string): string {
  let text = escaped
  // 代码块（```lang\n...\n```）— 先处理，内部不转义
  text = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
    return `<pre class="bg-[var(--bg-code)] text-[var(--text-code)] rounded p-2 my-1 overflow-x-auto text-xs"><code>${code}</code></pre>`
  })
  // 行内代码
  text = text.replace(/`([^`]+)`/g, '<code class="bg-[var(--bg-hover)] px-1 rounded text-xs">$1</code>')
  // 粗体
  text = text.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  // 斜体
  text = text.replace(/\*([^*]+)\*/g, '<em>$1</em>')
  // 换行
  text = text.replace(/\n/g, '<br>')
  return text
}

const renderedContent = computed(() => {
  const escaped = escapeHtml(props.message.content)
  return renderMarkdown(escaped)
})
</script>
