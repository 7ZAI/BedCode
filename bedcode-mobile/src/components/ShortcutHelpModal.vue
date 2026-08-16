<template>
  <Teleport to="body">
    <Transition name="bottom-sheet">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-help-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl modal-panel">
        <!-- Header -->
        <div class="flex items-center justify-between p-4 border-b border-[var(--mobile-border)]">
          <span class="font-semibold text-[var(--mobile-text-primary)] text-base">{{ $t('mobile.shortcutHelp.title') }}</span>
          <button
            class="p-1.5 rounded-lg hover:bg-[var(--mobile-accent-muted)] active:opacity-70 transition-colors"
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
import '@/styles/markdown-body.css'

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
