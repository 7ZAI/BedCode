<template>
  <!-- ChatGPT/DeepSeek 风格输入容器：模型切换与发送按钮同处输入框内 -->
  <div
    class="rounded-input border border-[var(--border-input)] bg-[var(--bg-card)] focus-within:border-brand focus-within:shadow-input-focus transition-colors"
  >
    <textarea
      ref="textareaRef"
      v-model="draft"
      rows="1"
      class="w-full resize-none min-h-[32px] max-h-40 bg-transparent px-3 pt-2.5 pb-1 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:outline-none"
      :placeholder="placeholder"
      :disabled="disabled"
      @keydown.enter.exact.prevent="onEnter"
      @keydown.enter.shift.prevent="insertNewline"
    ></textarea>

    <!-- 底部工具栏：左侧模型选择（父组件插槽），右侧发送/停止 -->
    <div class="flex items-center justify-between gap-2 px-2 pb-2 pt-0.5">
      <div class="flex items-center gap-1.5 min-w-0">
        <slot name="toolbar" />
      </div>

      <!-- 流式期间切换为停止按钮 -->
      <button
        v-if="streaming"
        class="w-8 h-8 flex-shrink-0 rounded-full flex items-center justify-center bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors"
        :title="t('desktop.plugin.aiChatbox.stop')"
        @click="$emit('stop')"
      >
        <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24">
          <rect x="6" y="6" width="12" height="12" rx="1.5" />
        </svg>
      </button>
      <!-- 发送按钮：空内容时置灰，与 ChatGPT 交互一致 -->
      <button
        v-else
        class="w-8 h-8 flex-shrink-0 rounded-full flex items-center justify-center transition-colors disabled:pointer-events-none"
        :class="canSend
          ? 'bg-brand text-[var(--color-primary-contrast)] hover:opacity-90'
          : 'bg-[var(--bg-hover)] text-[var(--text-tertiary)]'"
        :title="t('desktop.plugin.aiChatbox.send')"
        :disabled="disabled || !canSend"
        @click="send"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M7 17L17 7M17 7H8M17 7v9" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ChatInput — 多行输入框（ChatGPT/DeepSeek 风格容器）
 *
 * 容器内：上方自适应高度 textarea（Enter 发送 / Shift+Enter 换行，1~8 行）；
 * 底部工具栏左侧为模型选择（toolbar 插槽，由父组件注入 SDK Select），
 * 右侧圆形发送按钮（空内容置灰），流式期间切换为停止按钮。
 */
import { ref, computed, watch, nextTick } from 'vue'
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

/** 是否有可发送内容（控制发送按钮亮/灰） */
const canSend = computed(() => draft.value.trim().length > 0)

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

<style scoped>
/* SDK Select 在输入框内弱化为无边框圆角 chip（DeepSeek/ChatGPT 模型选择器样式），
   避免容器内出现嵌套输入框边框。hover 用 color-mix 朝文字色混入一档：
   浅色主题变深、深色主题变亮（--bg-input 与容器底色 --bg-card 相同，不能用于 hover） */
:deep(.model-picker .relative button) {
  height: 28px;
  padding: 0 8px 0 10px;
  background: var(--bg-hover);
  border-color: transparent;
  border-radius: 8px;
  box-shadow: none;
  transition: background-color 0.2s;
}
:deep(.model-picker .relative button:hover) {
  background: color-mix(in srgb, var(--bg-hover) 88%, var(--text-primary));
}
/* 小 chevron：覆盖 SDK 默认 w-5 h-5 */
:deep(.model-picker .relative button svg) {
  width: 14px;
  height: 14px;
}
/* 模型名过长时截断，保持 chip 稳定宽度（供应商名 / 模型名 展示空间） */
:deep(.model-picker .relative button span) {
  font-size: 12px;
  max-width: 230px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
