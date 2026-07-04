<template>
  <Teleport to="body">
    <Transition name="bottom-sheet">
    <div
      v-if="visible"
      class="fixed inset-0 z-[120] flex items-end justify-center mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-help-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl modal-panel">
        <!-- Header -->
        <div class="flex items-center justify-between p-4 border-b border-[var(--mobile-border)]">
          <span class="font-semibold text-[var(--mobile-text-primary)] text-base">{{ $t('mobile.shortcutHelp.title') }}</span>
          <button
            class="p-1.5 rounded-lg hover:bg-[var(--mobile-accent-muted)] transition-colors"
            @click="emit('close')"
          >
            <svg class="w-5 h-5 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- Markdown Content -->
        <div class="flex-1 overflow-y-auto p-4">
          <div class="md-body" v-html="renderedContent"></div>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 快捷键说明弹窗
 * 使用 marked 渲染静态 markdown 文档，展示各快捷键在终端中的功能
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { marked } from 'marked'
import zhCNHelp from '@/assets/shortcut-help.zh-CN.md?raw'
import enHelp from '@/assets/shortcut-help.en.md?raw'

const { locale } = useI18n()

defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const renderedContent = computed(() => {
  const md = locale.value === 'zh-CN' ? zhCNHelp : enHelp
  return marked.parse(md) as string
})
</script>

<style scoped>
/* Markdown 渲染样式 */
.md-body {
  font-size: 0.875rem;
  line-height: 1.7;
  color: var(--mobile-text-primary);
}

.md-body :deep(h1) {
  font-size: 1.25rem;
  font-weight: 700;
  color: var(--mobile-text-primary);
  margin-bottom: 1rem;
}

.md-body :deep(h2) {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-accent);
  margin-top: 1.25rem;
  margin-bottom: 0.5rem;
  padding-bottom: 0.25rem;
  border-bottom: 1px solid var(--mobile-border);
}

.md-body :deep(p) {
  margin: 0.5rem 0;
  color: var(--mobile-text-secondary);
}

.md-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 0.5rem 0 1rem;
  font-size: 0.8125rem;
}

.md-body :deep(thead th) {
  text-align: left;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-weight: 600;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-bottom: 2px solid var(--mobile-border);
}

.md-body :deep(tbody td) {
  padding: 0.4375rem 0.75rem;
  border-bottom: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

.md-body :deep(tbody tr:last-child td) {
  border-bottom: none;
}

.md-body :deep(tbody td:first-child) {
  font-family: 'Courier New', monospace;
  font-weight: 600;
  color: var(--mobile-accent);
  white-space: nowrap;
}

.md-body :deep(code) {
  font-family: 'Courier New', monospace;
  font-size: 0.8125rem;
  padding: 0.125rem 0.375rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.25rem;
  color: var(--mobile-accent);
}

.md-body :deep(strong) {
  color: var(--mobile-text-primary);
  font-weight: 600;
}

.md-body :deep(ul),
.md-body :deep(ol) {
  padding-left: 1.25rem;
  margin: 0.5rem 0;
  color: var(--mobile-text-secondary);
}

.md-body :deep(li) {
  margin: 0.25rem 0;
}

.md-body :deep(blockquote) {
  margin: 0.5rem 0;
  padding: 0.5rem 0.75rem;
  border-left: 3px solid var(--mobile-accent);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  border-radius: 0 0.375rem 0.375rem 0;
}
</style>
