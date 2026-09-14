<template>
  <div class="mde">
    <!-- 语法工具栏：mousedown.prevent 保证点击不抢 textarea 焦点，光标位置得以保留 -->
    <div v-if="toolbar" class="mde-toolbar" role="toolbar" :aria-label="t('toolbar')">
      <button
        v-for="action in toolbarActions"
        :key="action"
        type="button"
        class="mde-tb-btn"
        :disabled="disabled"
        :title="t(action)"
        :aria-label="t(action)"
        @mousedown.prevent
        @click="runAction(action)"
      >
        <span v-if="action === 'bold'" class="mde-tb-glyph mde-tb-bold">B</span>
        <span v-else-if="action === 'italic'" class="mde-tb-glyph mde-tb-italic">I</span>
        <span v-else-if="action === 'h1'" class="mde-tb-glyph">H1</span>
        <span v-else-if="action === 'h2'" class="mde-tb-glyph">H2</span>
        <span v-else-if="action === 'h3'" class="mde-tb-glyph">H3</span>
        <span v-else-if="action === 'olist'" class="mde-tb-glyph mde-tb-mono">1.</span>
        <span v-else-if="action === 'quote'" class="mde-tb-glyph mde-tb-mono">&quot;</span>
        <span v-else-if="action === 'codeInline'" class="mde-tb-glyph mde-tb-mono">`</span>
        <svg
          v-else
          class="mde-tb-icon"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          viewBox="0 0 24 24"
        >
          <template v-if="action === 'list'">
            <line x1="8" y1="6" x2="21" y2="6" />
            <line x1="8" y1="12" x2="21" y2="12" />
            <line x1="8" y1="18" x2="21" y2="18" />
            <line x1="3" y1="6" x2="3.01" y2="6" />
            <line x1="3" y1="12" x2="3.01" y2="12" />
            <line x1="3" y1="18" x2="3.01" y2="18" />
          </template>
          <template v-else-if="action === 'codeBlock'">
            <polyline points="16 18 22 12 16 6" />
            <polyline points="8 6 2 12 8 18" />
          </template>
          <template v-else-if="action === 'link'">
            <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
            <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
          </template>
          <template v-else-if="action === 'divider'">
            <line x1="3" y1="12" x2="21" y2="12" />
          </template>
        </svg>
      </button>

      <span class="mde-tb-sep"></span>

      <button
        v-if="preview"
        type="button"
        class="mde-tb-btn"
        :disabled="disabled"
        :title="mode === 'preview' ? t('edit') : t('preview')"
        :aria-label="mode === 'preview' ? t('edit') : t('preview')"
        @mousedown.prevent
        @click="toggleMode"
      >
        <svg
          v-if="mode === 'edit'"
          class="mde-tb-icon"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          viewBox="0 0 24 24"
        >
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
        <svg
          v-else
          class="mde-tb-icon"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          viewBox="0 0 24 24"
        >
          <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
        </svg>
      </button>
    </div>

    <!-- 编辑 / 预览主体（flex:1，随容器高度自适应） -->
    <div class="mde-body">
      <textarea
        v-show="mode === 'edit'"
        ref="textareaRef"
        class="mde-textarea"
        :value="draft"
        :placeholder="placeholder"
        :disabled="disabled"
        spellcheck="false"
        data-testid="markdown-textarea"
        @input="onInput"
      ></textarea>
      <div v-show="mode === 'preview'" class="mde-preview" data-testid="markdown-preview" v-html="previewHtml"></div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * MarkdownEditor - SDK 公共轻量 markdown 编辑器（textarea 内核 + 语法工具栏 + 预览切换）
 *
 * - 编辑内核：原生 <textarea>，v-model 契约（modelValue / update:modelValue）
 * - 语法插入：applyMarkdownSyntax 纯函数作用于光标/选区（src/ui/markdown-editor-syntax.ts）
 * - 预览渲染：marked（与移动端 FileViewerModal 同库同版本 18.0.11，渲染行为一致）
 * - 样式：宿主设计 token（--text-* / --bg-input / --border-input / --radius-input 等），
 *   暗色/亮色随 token 自动跟随；不硬编码色值
 * - i18n 中立：按钮 title 内置英文，可经 labels props 覆盖；不依赖 vue-i18n
 *
 * 安全取舍（有意记录）：预览不引入 DOMPurify（保持轻量，与移动端一致）；
 * 安全边界在 marked 渲染层收紧：
 * - 链接协议白名单（javascript: 等降级为纯文本）；
 * - **raw HTML 整体转义为文本**（marked 18 无 `html:false` 选项，关闭点是
 *   `renderer.html`）：预览内容可能是 GitHub/本地导入的**不可信 skill**，
 *   v-html 透传 raw HTML 即任意 XSS（`<img src=x onerror=...>` 等）。
 */
import { computed, ref, watch } from 'vue'
import { nextTick } from 'vue'
import { Marked } from 'marked'
import { applyMarkdownSyntax, type MarkdownSyntaxAction } from './markdown-editor-syntax'

export type MarkdownEditorLabelKey = MarkdownSyntaxAction | 'preview' | 'edit' | 'toolbar'

export interface MarkdownEditorProps {
  /** v-model 绑定值 */
  modelValue: string
  /** 空内容占位提示 */
  placeholder?: string
  /** 是否显示语法工具栏（默认 true） */
  toolbar?: boolean
  /** 是否显示预览切换按钮（默认 true） */
  preview?: boolean
  /** 禁用编辑与工具栏（保存中/预览锁定态） */
  disabled?: boolean
  /** 工具栏 title / 预览切换文案覆盖（不传用内置英文） */
  labels?: Partial<Record<MarkdownEditorLabelKey, string>>
}

const props = withDefaults(defineProps<MarkdownEditorProps>(), {
  placeholder: '',
  toolbar: true,
  preview: true,
  disabled: false,
  labels: () => ({}),
})

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

// ==================== i18n 中立的默认文案 ====================
const DEFAULT_LABELS: Record<MarkdownEditorLabelKey, string> = {
  bold: 'Bold',
  italic: 'Italic',
  h1: 'Heading 1',
  h2: 'Heading 2',
  h3: 'Heading 3',
  list: 'Bulleted list',
  olist: 'Numbered list',
  quote: 'Blockquote',
  codeBlock: 'Code block',
  codeInline: 'Inline code',
  link: 'Link',
  divider: 'Divider',
  preview: 'Preview',
  edit: 'Edit',
  toolbar: 'Markdown formatting',
}

function t(key: MarkdownEditorLabelKey): string {
  return props.labels?.[key] ?? DEFAULT_LABELS[key]
}

const toolbarActions: MarkdownSyntaxAction[] = [
  'bold',
  'italic',
  'h1',
  'h2',
  'h3',
  'list',
  'olist',
  'quote',
  'codeBlock',
  'codeInline',
  'link',
  'divider',
]

// ==================== 编辑状态 ====================
/** 草稿（组件内部持有；编辑/预览切换不丢内容） */
const draft = ref(props.modelValue)
const textareaRef = ref<HTMLTextAreaElement | null>(null)
const mode = ref<'edit' | 'preview'>('edit')

/** 外部 modelValue 变化时同步草稿（v-model 受控方向） */
watch(
  () => props.modelValue,
  (value) => {
    if (value !== draft.value) draft.value = value
  },
)

function onInput(event: Event) {
  draft.value = (event.target as HTMLTextAreaElement).value
  emit('update:modelValue', draft.value)
}

/** 工具栏动作：纯函数变换文本与光标/选区，恢复焦点与选区位置 */
function runAction(action: MarkdownSyntaxAction) {
  const ta = textareaRef.value
  if (!ta) return
  const result = applyMarkdownSyntax(action, draft.value, ta.selectionStart, ta.selectionEnd)
  draft.value = result.text
  emit('update:modelValue', result.text)
  void nextTick(() => {
    ta.focus()
    ta.setSelectionRange(result.selectionStart, result.selectionEnd)
  })
}

function toggleMode() {
  mode.value = mode.value === 'edit' ? 'preview' : 'edit'
}

// ==================== 预览渲染（marked） ====================

/** 链接协议白名单：只允许 http/https/mailto/锚点/相对路径，未知协议降级为纯文本 */
function isSafeUrl(href: string): boolean {
  if (!href) return false
  if (/^(https?:|mailto:|#)/i.test(href)) return true
  // 相对路径（不以协议形式开头）
  return !/^[a-z][a-z0-9+.-]*:/i.test(href)
}

function escapeAttr(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')
}

/** HTML 实体转义：raw HTML 降级为可见文本，不生成可执行节点 */
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

// 组件私有 Marked 实例（模块级构建一次，多实例组件共享同一渲染策略）：
// 不用全局 `marked` 单例的 use() 副作用——vitest/多副本解析下全局注册可能落在
// 另一份 marked 实例上（实测组件渲染与注册策略分家），私有实例保证
// 「注册的安全策略 = 实际渲染路径」恒一致。
const md = new Marked()
md.use({
  renderer: {
    link({ href, title, tokens }) {
      const text = this.parser.parseInline(tokens)
      if (!isSafeUrl(href)) return text
      const attrs = [`href="${escapeAttr(href)}"`, 'target="_blank"', 'rel="noopener noreferrer"']
      if (title) attrs.push(`title="${escapeAttr(title)}"`)
      return `<a ${attrs.join(' ')}>${text}</a>`
    },
    // 块级与内联 raw HTML 统一转义为文本：marked 默认透传 raw HTML，
    // 预览内容含不可信来源时等于任意 XSS（实测 <img onerror> 原样进 v-html）。
    // marked 18 已无 `html:false` 选项，安全关闭点即本 renderer。
    html({ text }) {
      return escapeHtml(text)
    },
  },
})

const previewHtml = computed(() => md.parse(draft.value) as string)
</script>

<style scoped>
/* ==================== 容器 ==================== */
.mde {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  box-sizing: border-box;
}

/* ==================== 工具栏 ==================== */
.mde-toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 6px 8px;
  border: 1px solid var(--border-input);
  border-bottom: none;
  border-radius: var(--radius-input) var(--radius-input) 0 0;
  background: var(--bg-input);
  flex-shrink: 0;
}

.mde-tb-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.mde-tb-btn:hover {
  background: var(--color-primary-light);
  color: var(--text-primary);
}

.mde-tb-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.mde-tb-btn:disabled:hover {
  background: transparent;
  color: var(--text-secondary);
}

.mde-tb-btn:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: -2px;
}

.mde-tb-icon {
  width: 15px;
  height: 15px;
  flex-shrink: 0;
}

.mde-tb-glyph {
  font-size: 13px;
  line-height: 1;
  font-family: inherit;
}

.mde-tb-bold {
  font-weight: 700;
}

.mde-tb-italic {
  font-style: italic;
}

.mde-tb-mono {
  font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
}

.mde-tb-sep {
  width: 1px;
  height: 16px;
  margin: 0 4px;
  background: var(--border-input);
  flex-shrink: 0;
}

/* ==================== 编辑 / 预览主体 ==================== */
.mde-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-input);
  border-top: none;
  border-radius: 0 0 var(--radius-input) var(--radius-input);
  background: var(--bg-input);
}

.mde-textarea {
  flex: 1;
  min-height: 0;
  width: 100%;
  height: 100%;
  padding: 10px 12px;
  box-sizing: border-box;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: var(--text-primary);
  font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
  font-size: var(--font-size-base);
  line-height: 1.6;
}

.mde-textarea::placeholder {
  color: var(--text-tertiary);
}

.mde-textarea:disabled {
  cursor: not-allowed;
}

/* 预览区：基础 markdown 排版，色值全部走宿主 token */
.mde-preview {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 12px;
  color: var(--text-primary);
  font-size: var(--font-size-base);
  line-height: 1.6;
  word-break: break-word;
}

.mde-preview :deep(h1),
.mde-preview :deep(h2),
.mde-preview :deep(h3),
.mde-preview :deep(h4),
.mde-preview :deep(h5),
.mde-preview :deep(h6) {
  margin: 0.6em 0 0.4em;
  line-height: 1.3;
  font-weight: 600;
  color: var(--text-primary);
}

.mde-preview :deep(h1) {
  font-size: 1.5em;
}

.mde-preview :deep(h2) {
  font-size: 1.3em;
}

.mde-preview :deep(h3) {
  font-size: 1.15em;
}

.mde-preview :deep(p) {
  margin: 0.4em 0;
}

.mde-preview :deep(ul),
.mde-preview :deep(ol) {
  margin: 0.4em 0;
  padding-left: 1.6em;
}

.mde-preview :deep(li) {
  margin: 0.15em 0;
}

.mde-preview :deep(blockquote) {
  margin: 0.5em 0;
  padding: 0.2em 0.9em;
  border-left: 3px solid var(--border-strong);
  color: var(--text-secondary);
}

.mde-preview :deep(code) {
  padding: 0.15em 0.35em;
  border-radius: 4px;
  background: var(--bg-hover);
  font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
  font-size: 0.9em;
}

.mde-preview :deep(pre) {
  margin: 0.5em 0;
  padding: 0.7em 0.9em;
  overflow-x: auto;
  border-radius: var(--radius-input);
  background: var(--bg-hover);
}

.mde-preview :deep(pre code) {
  padding: 0;
  background: transparent;
}

.mde-preview :deep(a) {
  color: var(--color-primary);
  text-decoration: underline;
}

.mde-preview :deep(hr) {
  margin: 0.8em 0;
  border: none;
  border-top: 1px solid var(--border-strong);
}

.mde-preview :deep(table) {
  margin: 0.5em 0;
  border-collapse: collapse;
}

.mde-preview :deep(th),
.mde-preview :deep(td) {
  padding: 0.3em 0.7em;
  border: 1px solid var(--border-input);
}

.mde-preview :deep(img) {
  max-width: 100%;
}
</style>
