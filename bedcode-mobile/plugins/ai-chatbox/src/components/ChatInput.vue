<template>
  <!-- 输入框容器（DeepSeek/Claude 式）：textarea 在上，底部一行 = 左下模型 pill + 右下圆形发送/停止 -->
  <div
    class="rounded-2xl border border-[var(--mobile-input-border)] bg-[var(--mobile-input-bg)] px-2.5 py-2 focus-within:border-[var(--mobile-input-focus)] transition-colors"
  >
    <textarea
      ref="textareaRef"
      v-model="draft"
      rows="1"
      class="w-full resize-none min-h-[44px] max-h-40 px-1 text-[var(--font-size-base)] leading-snug bg-transparent text-[var(--mobile-text-primary)] placeholder:text-[var(--mobile-input-placeholder)] focus:outline-none"
      :placeholder="placeholder"
      :disabled="disabled"
      @keydown.enter.exact.prevent="onEnter"
      @keydown.enter.shift.prevent="insertNewline"
    ></textarea>

    <!-- 底部一行：左下模型 pill（Select sm）+ 右下圆形发送/停止 -->
    <div class="flex items-center justify-between mt-1 -mx-1 min-h-9">
      <Select
        v-if="showModel && modelOptions.length > 0"
        :model-value="modelValue"
        :options="modelOptions"
        size="sm"
        placement="top"
        :placeholder="t('mobile.plugin.aiChatbox.model')"
        class="max-w-[13rem]"
        @update:model-value="emit('update:modelValue', String($event))"
      />
      <span v-else></span>

      <button
        v-if="streaming"
        class="w-10 h-10 flex-shrink-0 rounded-full flex items-center justify-center bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity"
        :title="t('mobile.plugin.aiChatbox.stop')"
        @click="emit('stop')"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
          <rect x="6" y="6" width="12" height="12" rx="2" />
        </svg>
      </button>
      <button
        v-else
        class="w-10 h-10 flex-shrink-0 rounded-full flex items-center justify-center transition-colors active:opacity-80 disabled:pointer-events-none"
        :class="canSend
          ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'
          : 'bg-[var(--mobile-bg-tertiary)] text-[var(--mobile-text-disabled)]'"
        :title="t('mobile.plugin.aiChatbox.send')"
        :disabled="!canSend"
        @click="send"
      >
        <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19V5" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12l7-7 7 7" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ChatInput — 多行输入框（移动端，DeepSeek/Claude 式内联布局）
 *
 * 输入框容器内：textarea 在上；底部一行左下角模型切换 pill（Select sm）、
 * 右下角圆形发送按钮（空输入灰底禁用）/ 流式时切换为停止按钮。
 * Enter 发送 / Shift+Enter 换行；textarea 自适应高度（1~8 行）。
 */
import { ref, computed, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import Select from '@binblink/plugin-sdk-mobile/ui'

const props = withDefaults(defineProps<{
  disabled?: boolean
  streaming?: boolean
  placeholder?: string
  /** 当前模型（pill 显示） */
  modelValue?: string
  /** 模型选项 */
  modelOptions?: { value: string | number; label: string }[]
  /** 是否显示模型 pill（无供应商时不显示） */
  showModel?: boolean
}>(), {
  modelValue: '',
  modelOptions: () => [],
  showModel: true,
})

const emit = defineEmits<{
  send: [content: string]
  stop: []
  'update:modelValue': [value: string]
}>()

const { t } = useI18n()

const draft = ref('')
const textareaRef = ref<HTMLTextAreaElement | null>(null)

const canSend = computed(() => !props.disabled && draft.value.trim() !== '')

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
