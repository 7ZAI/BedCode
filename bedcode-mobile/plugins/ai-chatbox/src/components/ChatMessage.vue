<template>
  <!-- user 消息右对齐，assistant 消息左对齐（flex-row-reverse 实现左右分列） -->
  <div class="flex gap-2.5" :class="isUser ? 'flex-row-reverse' : ''">
    <!-- 头像 -->
    <div
      class="w-9 h-9 rounded-xl flex items-center justify-center flex-shrink-0 text-sm mt-0.5"
      :class="isUser
        ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'
        : 'bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)]'"
    >
      <svg v-if="isUser" class="w-4.5 h-4.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
      </svg>
      <span v-else>{{ t('mobile.plugin.aiChatbox.assistant') }}</span>
    </div>

    <div class="flex-1 min-w-0 space-y-1" :class="isUser ? 'text-right' : ''">
      <!-- 元信息行（user 反向排列，名称靠右端） -->
      <div class="flex items-center gap-2 text-xs text-[var(--mobile-text-muted)]" :class="isUser ? 'flex-row-reverse' : ''">
        <span>{{ isUser ? t('mobile.plugin.aiChatbox.you') : t('mobile.plugin.aiChatbox.assistant') }}</span>
        <span v-if="message.model" class="font-mono">{{ message.model }}</span>
        <!-- token 用量 -->
        <span v-if="message.usage" class="font-mono rounded-md bg-[var(--mobile-bg-tertiary)] px-1.5 py-0.5">
          ↑{{ message.usage.promptTokens }} ↓{{ message.usage.completionTokens }} Σ{{ message.usage.totalTokens }}
        </span>
      </div>

      <!-- 内容（user 文本右对齐，assistant 保持左对齐） -->
      <div
        v-if="isUser"
        class="whitespace-pre-wrap break-words text-[var(--font-size-base)] leading-relaxed text-[var(--mobile-text-primary)]"
      >{{ message.content }}</div>
      <div v-else ref="contentRef" class="text-[var(--font-size-base)] leading-relaxed text-[var(--mobile-text-primary)] md-body" v-html="rendered" />

      <!-- 错误提示（assistant 无内容且带错误时） -->
      <div
        v-if="!isUser && !message.content && errorText"
        class="text-xs text-[var(--mobile-error)]"
      >{{ errorText }}</div>

      <!-- 操作条（移动端无 hover，常显：复制 / 删除） -->
      <div class="flex items-center gap-1" :class="isUser ? 'justify-end' : ''">
        <button
          class="h-9 min-w-11 px-2.5 inline-flex items-center justify-center gap-1 text-xs text-[var(--mobile-text-muted)] bg-[var(--mobile-bg-tertiary)] rounded-lg active:opacity-80 transition-opacity"
          @click="copyContent"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3" />
          </svg>
          {{ t('mobile.plugin.aiChatbox.copy') }}
        </button>
        <button
          class="h-9 min-w-11 px-2.5 inline-flex items-center justify-center gap-1 text-xs text-[var(--mobile-text-muted)] bg-[var(--mobile-bg-tertiary)] rounded-lg active:opacity-80 transition-opacity"
          @click="$emit('delete', message)"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          {{ t('mobile.plugin.aiChatbox.delete') }}
        </button>
      </div>

      <!-- 流式光标 -->
      <span
        v-if="streaming"
        class="inline-block w-2 h-4 ml-0.5 align-middle bg-[var(--mobile-accent)] animate-pulse"
      ></span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ChatMessage — 单条聊天消息（移动端）
 *
 * user 消息纯文本；assistant 消息 Markdown 渲染（marked + highlight.js 代码高亮），
 * 支持整条复制、代码块一键复制、删除、token 用量显示、流式光标。
 * 移动端无 hover：复制/删除为常显操作条。
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import hljs from 'highlight.js'
import type { ChatMessage } from '../types'

const props = defineProps<{
  message: ChatMessage
  streaming?: boolean
  errorText?: string
}>()

defineEmits<{ delete: [message: ChatMessage] }>()

const { t } = useI18n()

const isUser = computed(() => props.message.role === 'user')

/** Markdown → HTML（breaks 让单换行也换行，贴合聊天场景）
 *
 * LLM 输出不可信：marked 保留原始 HTML，prompt injection 可注入
 * `<img onerror>` 等脚本在插件上下文执行（插件持宿主命令桥接能力），
 * 必须经 DOMPurify 消毒后再进 v-html */
const rendered = computed(() => {
  const html = marked.parse(props.message.content, { async: false, breaks: true }) as string
  return DOMPurify.sanitize(html)
})

const contentRef = ref<HTMLElement | null>(null)

/** 代码块高亮 + 注入一键复制按钮（渲染后执行；按钮幂等避免重复注入） */
function enhanceCodeBlocks(): void {
  const container = contentRef.value
  if (!container) return
  container.querySelectorAll<HTMLElement>('pre').forEach(pre => {
    const code = pre.querySelector('code')
    if (code) {
      hljs.highlightElement(code)
    }
    if (pre.querySelector('.md-copy-btn')) return
    const btn = document.createElement('button')
    btn.className = 'md-copy-btn'
    btn.textContent = t('mobile.plugin.aiChatbox.copy')
    btn.addEventListener('click', () => {
      const text = pre.querySelector('code')?.innerText ?? ''
      navigator.clipboard.writeText(text).catch(() => {})
    })
    pre.appendChild(btn)
  })
}

/** 复制整条消息 */
async function copyContent(): Promise<void> {
  await navigator.clipboard.writeText(props.message.content).catch(() => {})
}

onMounted(enhanceCodeBlocks)
watch(() => props.message.content, enhanceCodeBlocks)
</script>

<style scoped>
/* Markdown 正文样式（v-html 内容无 scoped 类，用 :deep 穿透） */
/* highlight.js 语法高亮配色：低饱和、与宿主 Dracula 系调色板协调。
   移动端默认深色（:root），浅色主题作用于 html:not(.dark)（两套配色） */
.md-body :deep(.hljs-comment),
.md-body :deep(.hljs-quote) {
  color: var(--mobile-text-muted);
  font-style: italic;
}
.md-body :deep(.hljs-keyword),
.md-body :deep(.hljs-selector-tag) { color: #e08a6a; }
.md-body :deep(.hljs-type),
.md-body :deep(.hljs-class) { color: #7db8b0; }
.md-body :deep(.hljs-string),
.md-body :deep(.hljs-attr),
.md-body :deep(.hljs-template-variable) { color: #a8c080; }
.md-body :deep(.hljs-number),
.md-body :deep(.hljs-literal) { color: #e0a06a; }
.md-body :deep(.hljs-title),
.md-body :deep(.hljs-function) { color: #d9b06a; }
.md-body :deep(.hljs-built_in) { color: #c99ab8; }
.md-body :deep(.hljs-meta) { color: var(--mobile-text-secondary); }

/* 浅色主题（html 无 .dark 时）覆盖为暖色低饱和值 */
:global(html:not(.dark)) .md-body :deep(.hljs-comment),
:global(html:not(.dark)) .md-body :deep(.hljs-quote) { color: #6b7280; }
:global(html:not(.dark)) .md-body :deep(.hljs-keyword),
:global(html:not(.dark)) .md-body :deep(.hljs-selector-tag) { color: #8a3b2e; }
:global(html:not(.dark)) .md-body :deep(.hljs-type),
:global(html:not(.dark)) .md-body :deep(.hljs-class) { color: #2f6f6a; }
:global(html:not(.dark)) .md-body :deep(.hljs-string),
:global(html:not(.dark)) .md-body :deep(.hljs-attr),
:global(html:not(.dark)) .md-body :deep(.hljs-template-variable) { color: #5a7a2f; }
:global(html:not(.dark)) .md-body :deep(.hljs-number),
:global(html:not(.dark)) .md-body :deep(.hljs-literal) { color: #a05a2c; }
:global(html:not(.dark)) .md-body :deep(.hljs-title),
:global(html:not(.dark)) .md-body :deep(.hljs-function) { color: #8a5a1d; }
:global(html:not(.dark)) .md-body :deep(.hljs-built_in) { color: #7a4a6b; }
:global(html:not(.dark)) .md-body :deep(.hljs-meta) { color: var(--mobile-text-secondary); }

.md-body :deep(h1),
.md-body :deep(h2),
.md-body :deep(h3),
.md-body :deep(h4) {
  font-weight: 600;
  margin: 0.75em 0 0.375em;
  line-height: 1.3;
}
.md-body :deep(h1) { font-size: 1.25em; }
.md-body :deep(h2) { font-size: 1.125em; }
.md-body :deep(h3) { font-size: 1em; }
.md-body :deep(p) { margin: 0.375em 0; }
.md-body :deep(ul),
.md-body :deep(ol) {
  margin: 0.375em 0;
  padding-left: 1.5em;
  list-style: revert;
}
.md-body :deep(li) { margin: 0.125em 0; }
.md-body :deep(a) {
  color: var(--mobile-accent);
  text-decoration: underline;
}
.md-body :deep(blockquote) {
  border-left: 3px solid var(--mobile-border);
  padding-left: 0.75em;
  color: var(--mobile-text-secondary);
  margin: 0.375em 0;
}
.md-body :deep(code) {
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 0.875em;
  background: var(--mobile-bg-tertiary);
  padding: 0.125em 0.375em;
  border-radius: 0.25rem;
}
.md-body :deep(pre) {
  position: relative;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  padding: 0.75rem;
  padding-top: 1.75rem;
  overflow-x: auto;
  margin: 0.5em 0;
}
.md-body :deep(pre code) {
  background: transparent;
  padding: 0;
  font-size: 0.8125rem;
  line-height: 1.6;
}
.md-body :deep(pre .md-copy-btn) {
  position: absolute;
  top: 0.375rem;
  right: 0.5rem;
  font-size: 0.6875rem;
  color: var(--mobile-text-muted);
  background: transparent;
  border: 1px solid var(--mobile-border);
  border-radius: 0.25rem;
  padding: 0.125rem 0.5rem;
  cursor: pointer;
  transition: color 0.15s;
}
.md-body :deep(pre .md-copy-btn:active) {
  color: var(--mobile-text-secondary);
}
.md-body :deep(table) {
  border-collapse: collapse;
  margin: 0.5em 0;
}
.md-body :deep(th),
.md-body :deep(td) {
  border: 1px solid var(--mobile-border);
  padding: 0.375em 0.75em;
  text-align: left;
}
.md-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--mobile-border);
  margin: 0.75em 0;
}
</style>
