<template>
  <Teleport to="body">
    <transition name="center-modal">
      <div v-if="visible" class="viewer-overlay mobile-ui" @click.self="handleClose" @touchstart.stop @touchmove.stop>
        <div class="viewer-modal modal-panel" :class="{ 'viewer-fullscreen': isFullscreen }" :style="modalStyle">
        <!-- Header -->
        <div class="viewer-header">
          <div class="viewer-title-area">
            <span class="viewer-filename">{{ filename }}</span>
            <span class="viewer-lang-badge">{{ displayLang }}</span>
          </div>
          <div class="viewer-actions">
            <!-- Markdown 预览/源码切换（仅 .md 文件显示） -->
            <button v-if="isMarkdownFile" class="viewer-action-btn viewer-mode-btn" :class="{ active: viewMode === 'preview' }" :title="t('mobile.file.previewMode')" @click="viewMode = 'preview'">
              <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
              </svg>
            </button>
            <button v-if="isMarkdownFile" class="viewer-action-btn viewer-mode-btn" :class="{ active: viewMode === 'source' }" :title="t('mobile.file.sourceMode')" @click="viewMode = 'source'">
              <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
              </svg>
            </button>
            <button class="viewer-action-btn" :title="t('mobile.file.settingsTitle')" @click="showSettings = true">
              <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </button>
            <button class="viewer-action-btn" :title="isFullscreen ? t('mobile.file.exitFullscreen') : t('mobile.file.fullscreen')" @click="toggleFullscreen">
              <svg v-if="!isFullscreen" width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
              </svg>
              <svg v-else width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25" />
              </svg>
            </button>
            <button class="viewer-action-btn" :title="t('mobile.file.close')" @click="handleClose">
              <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Code Area -->
        <div class="viewer-body">
          <div v-if="loading" class="viewer-loading">{{ t('mobile.file.loading') }}</div>
          <div v-else-if="error" class="viewer-error">{{ error }}</div>
          <div v-else-if="!code && !diffLines?.length" class="viewer-loading">{{ t('mobile.file.selectFile') }}</div>
          <!-- Markdown 预览模式 -->
          <div
            v-else-if="isMarkdownFile && viewMode === 'preview'"
            class="viewer-md-preview"
            v-html="renderedMarkdown"
          ></div>
          <!-- 源码模式（shiki 高亮） -->
          <div
            v-else-if="highlightedHtml"
            class="viewer-code"
            :class="{ 'hide-line-numbers': !codeViewerStore.settings.showLineNumbers }"
            :style="codeStyle"
            v-html="highlightedHtml"
          ></div>
        </div>

        <!-- Footer -->
        <div class="viewer-footer">
          <span class="viewer-lang-label">{{ displayLang }}</span>
          <span class="viewer-line-count">{{ t('mobile.file.lineCount', { count: lineCount }) }}</span>
        </div>
      </div>
    </div>
  </transition>
  </Teleport>

  <!-- Settings Modal -->
  <CodeViewerSettingsModal
    :visible="showSettings"
    :z-index="60"
    @close="showSettings = false"
    @confirm="showSettings = false"
  />
</template>

<script setup lang="ts">
import { ref, computed, watch, inject, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { marked } from 'marked'
import { useCodeHighlight, getLangByFilename } from '@/composables/useCodeHighlight'
import type { FileDiffLine } from '@/composables/useHttpApi'
import { useCodeViewerStore, resolveCodeTheme, CODE_THEMES } from '@/stores/codeViewer'
import { useTheme } from '@/composables/useTheme'
import CodeViewerSettingsModal from '@/components/CodeViewerSettingsModal.vue'

const { t } = useI18n()
const { isSystemDark } = useTheme()

const props = defineProps<{
  visible: boolean
  filename: string
  code?: string
  diffLines?: FileDiffLine[]
  loading?: boolean
  error?: string | null
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
}>()

const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!
const { highlightedHtml, isLoading, error, highlight, highlightDiff } = useCodeHighlight()

const isFullscreen = ref(false)
const codeViewerStore = useCodeViewerStore()
const showSettings = ref(false)
/** Markdown 文件预览/源码切换：默认预览 */
const viewMode = ref<'preview' | 'source'>('preview')

/** 解析当前实际使用的 shiki 主题 ID */
const resolvedTheme = computed(() => resolveCodeTheme(codeViewerStore.settings.theme, isSystemDark.value))

/** 代码区域背景色：使用 shiki 主题的 background 色值 */
const codeBgColor = computed(() => {
  const themeConfig = CODE_THEMES[resolvedTheme.value]
  return themeConfig?.background ?? 'var(--mobile-bg-secondary)'
})

const isMarkdownFile = computed(() => {
  const ext = props.filename.split('.').pop()?.toLowerCase() || ''
  return ext === 'md' || ext === 'mdx'
})

const renderedMarkdown = computed(() => {
  if (!props.code || !isMarkdownFile.value) return ''
  return marked.parse(props.code) as string
})

const displayLang = computed(() => getLangByFilename(props.filename))

const lineCount = computed(() => {
  if (props.diffLines?.length) return props.diffLines.length
  const content = props.code ?? ''
  if (!content) return 0
  return content.split('\n').length
})

/** 行号列宽度：根据总行数自适应，确保行号完整展示 */
const gutterWidth = computed(() => {
  const digits = String(lineCount.value).length
  // 每位数字约 0.6em + 右侧 padding 0.6em + 左右留白 0.4em
  return `${digits * 0.6 + 1.0}em`
})

const codeStyle = computed(() => ({
  '--code-font-size': `${codeViewerStore.settings.fontSize}px`,
  '--code-line-height': codeViewerStore.settings.lineHeight,
  '--code-tab-size': codeViewerStore.settings.tabSize,
  '--code-bg': codeBgColor.value,
  '--code-gutter-width': gutterWidth.value,
}))

const modalStyle = computed(() => {
  if (isFullscreen.value) {
    return {
      paddingTop: `${safeArea.value.top}px`,
      paddingBottom: `${safeArea.value.bottom}px`,
    }
  }
  return {}
})

function toggleFullscreen() {
  isFullscreen.value = !isFullscreen.value
}

function handleClose() {
  isFullscreen.value = false
  emit('update:visible', false)
}

/** 使用解析后的主题执行高亮 */
async function doHighlight() {
  if (!props.visible || !props.filename) return
  const lang = getLangByFilename(props.filename)
  const theme = resolvedTheme.value
  if (props.diffLines && props.diffLines.length > 0) {
    await highlightDiff(props.diffLines, lang, theme, props.code)
  } else if (props.code) {
    await highlight(props.code, lang, theme)
  }
}

// 当文件变化时重新高亮
watch(
  () => [props.visible, props.filename, props.code, props.diffLines] as const,
  async ([visible]) => {
    if (!visible) return
    // 切换文件时重置为预览模式
    viewMode.value = 'preview'
    await doHighlight()
  },
  { immediate: true },
)

// 监听主题设置或系统暗色模式变化，重新高亮
watch(
  [() => codeViewerStore.settings.theme, isSystemDark],
  () => doHighlight(),
)
</script>

<style scoped>
.viewer-overlay {
  position: fixed;
  inset: 0;
  background: var(--mobile-overlay-heavy);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
  padding: 1rem;
}

.viewer-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 90%;
  max-width: clamp(280px, 700px, 900px);
  height: 80vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.viewer-fullscreen {
  width: 100%;
  height: 100%;
  max-width: none;
  border-radius: 0;
}

.viewer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

.viewer-title-area {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.viewer-filename {
  font-size: var(--font-size-base);
  font-weight: 600;
  color: var(--mobile-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.viewer-lang-badge {
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
  text-transform: uppercase;
  flex-shrink: 0;
}

.viewer-actions {
  display: flex;
  gap: 0.25rem;
  flex-shrink: 0;
}

.viewer-action-btn {
  padding: clamp(0.25rem, 0.375rem, 0.5rem);
  border-radius: 0.375rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s ease;
}

.viewer-action-btn:hover {
  color: var(--mobile-text-primary);
  background: var(--mobile-bg-elevated);
}

.viewer-action-btn:active {
  color: var(--mobile-accent);
}

.viewer-body {
  flex: 1;
  overflow: auto;
  -webkit-overflow-scrolling: touch;
  padding: 0;

  scrollbar-width: thin;
  scrollbar-color: var(--mobile-border) transparent;
}

/* 代码查看时，滚动容器背景跟随代码主题色，避免横向滚动时右侧空白 */
.viewer-body:has(.viewer-code) {
  background: var(--code-bg, var(--mobile-bg-secondary));
}

.viewer-body::-webkit-scrollbar {
  width: 4px;
  height: 4px;
}

.viewer-body::-webkit-scrollbar-track {
  background: transparent;
}

.viewer-body::-webkit-scrollbar-thumb {
  background: var(--mobile-border);
  border-radius: 2px;
}

/* ==================== VS Code 风格代码区域 ==================== */

.viewer-code {
  margin: 0;
  padding: 0.75rem 0 0.5rem;
  font-size: var(--code-font-size, 13px);
  line-height: var(--code-line-height, 1.5);
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  tab-size: var(--code-tab-size, 4);
  background: var(--code-bg, var(--mobile-bg-secondary));
  /* inline-block + min-width: 100% 保证长行横向滚动时背景延伸覆盖 */
  display: inline-block;
  min-width: 100%;
  box-sizing: border-box;
}

/* Shiki 产出的 pre — 重置为容器角色 */
.viewer-code :deep(pre) {
  margin: 0;
  padding: 0;
  background: transparent !important;
}

/* Shiki 产出的 code — 整体布局 */
.viewer-code :deep(code) {
  font-family: inherit;
  font-size: inherit;
  line-height: inherit;
  display: block;
  padding: 0;
}

/* ==================== 行布局：gutter + 代码 ==================== */

.viewer-code :deep(.line) {
  display: block;
  position: relative;
  padding-left: var(--code-gutter-width, 2.8em);
  white-space: pre;
}

/* 行号区 (gutter) — VS Code 风格：独立背景 + 右侧分隔线 */
.viewer-code :deep(.line::before) {
  content: attr(data-line);
  position: absolute;
  left: 0;
  top: 0;
  width: var(--code-gutter-width, 2.8em);
  padding-right: 0.6em;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  color: var(--mobile-code-gutter-color);
  font-size: inherit;
  line-height: inherit;
  user-select: none;
  pointer-events: none;
  /* 行号区域需要不透明背景遮挡下方代码文本 */
  background: var(--code-bg, var(--mobile-bg-secondary));
  border-right: 1px solid var(--mobile-code-gutter-border);
  z-index: 1;
}

/* 空行保持行高 */
.viewer-code :deep(.line:empty::after) {
  content: '\00a0';
}

/* ==================== Diff 行样式 ==================== */

.viewer-code :deep(.diff-line) {
  display: flex;
  align-items: stretch;
  line-height: 1.5;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  font-size: var(--code-font-size, 13px);
  white-space: pre;
  background: var(--code-bg, var(--mobile-bg-secondary));
}

.viewer-code :deep(.diff-line-no) {
  width: var(--code-gutter-width, 2.8em);
  padding: 0 0.6em;
  text-align: right;
  font-size: 0.85em;
  user-select: none;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  /* 行号区域需要不透明背景遮挡下方代码文本 */
  background: var(--code-bg, var(--mobile-bg-secondary));
  border-right: 1px solid var(--mobile-code-gutter-border);
  position: sticky;
  left: 0;
  z-index: 1;
}

.viewer-code :deep(.diff-old-no) {
  color: rgba(248, 81, 73, 0.6);
}

.viewer-code :deep(.diff-new-no) {
  color: rgba(63, 185, 80, 0.6);
  position: sticky;
  left: var(--code-gutter-width, 2.8em);
}

.viewer-code :deep(.diff-marker) {
  width: 1.2em;
  text-align: center;
  font-size: 0.85em;
  user-select: none;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  position: sticky;
  left: calc(var(--code-gutter-width, 2.8em) * 2);
  z-index: 1;
}

.viewer-code :deep(.diff-content) {
  flex: 1;
  min-width: 0;
  padding-left: 0.5em;
}

.viewer-code :deep(.diff-removed) {
  background: rgba(248, 81, 73, 0.15);
}
.viewer-code :deep(.diff-removed .diff-marker) {
  color: rgba(220, 38, 38, 0.9);
}
.viewer-code :deep(.diff-removed .diff-new-no) {
  background: rgba(248, 81, 73, 0.08);
}

.viewer-code :deep(.diff-added) {
  background: rgba(63, 185, 80, 0.15);
}
.viewer-code :deep(.diff-added .diff-marker) {
  color: rgba(5, 150, 105, 0.9);
}
.viewer-code :deep(.diff-added .diff-old-no) {
  background: rgba(63, 185, 80, 0.08);
}

.viewer-code :deep(.diff-context .diff-line-no) {
  color: var(--mobile-code-gutter-color);
}

/* 行号隐藏 */
.viewer-code.hide-line-numbers :deep(.line) {
  padding-left: 0.75em;
}

.viewer-code.hide-line-numbers :deep(.line::before) {
  content: none;
}

.viewer-code.hide-line-numbers :deep(.diff-line-no) {
  display: none;
}

.viewer-loading,
.viewer-error {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--mobile-text-muted);
  font-size: var(--font-size-base);
}

.viewer-error {
  color: var(--mobile-error);
}

/* ==================== 模式切换按钮 ==================== */

.viewer-mode-btn.active {
  color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
}

/* ==================== Markdown 预览 ==================== */

.viewer-md-preview {
  padding: 1rem;
  font-size: var(--font-size-base);
  line-height: 1.7;
  color: var(--mobile-text-primary);
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  word-break: break-word;
}

.viewer-md-preview :deep(h1) {
  font-size: clamp(1.25rem, 1.375rem + (100vw - 360px) / 840 * 2, 1.5rem);
  font-weight: 700;
  color: var(--mobile-text-primary);
  margin: 0 0 0.75rem;
  padding-bottom: 0.375rem;
  border-bottom: 1px solid var(--mobile-border);
}

.viewer-md-preview :deep(h2) {
  font-size: var(--font-size-xl);
  font-weight: 600;
  color: var(--mobile-accent);
  margin: 1.25rem 0 0.5rem;
  padding-bottom: 0.25rem;
  border-bottom: 1px solid var(--mobile-border);
}

.viewer-md-preview :deep(h3) {
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 1rem 0 0.375rem;
}

.viewer-md-preview :deep(p) {
  margin: 0.5rem 0;
  color: var(--mobile-text-secondary);
}

.viewer-md-preview :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 0.5rem 0 1rem;
  font-size: var(--font-size-sm);
  display: block;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}

.viewer-md-preview :deep(thead th) {
  text-align: left;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-weight: 600;
  font-size: var(--font-size-sm);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-bottom: 2px solid var(--mobile-border);
  white-space: nowrap;
}

.viewer-md-preview :deep(tbody td) {
  padding: 0.4375rem 0.75rem;
  border-bottom: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

.viewer-md-preview :deep(tbody tr:last-child td) {
  border-bottom: none;
}

.viewer-md-preview :deep(code) {
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  font-size: var(--font-size-sm);
  padding: 0.125rem 0.375rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.25rem;
  color: var(--mobile-accent);
}

.viewer-md-preview :deep(pre) {
  margin: 0.75rem 0;
  padding: 0.75rem 1rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.5rem;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}

.viewer-md-preview :deep(pre code) {
  padding: 0;
  background: none;
  border: none;
  border-radius: 0;
  font-size: var(--font-size-sm);
  color: var(--mobile-text-primary);
}

.viewer-md-preview :deep(blockquote) {
  margin: 0.75rem 0;
  padding: 0.5rem 0.75rem;
  border-left: 3px solid var(--mobile-accent);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  border-radius: 0 0.375rem 0.375rem 0;
}

.viewer-md-preview :deep(ul),
.viewer-md-preview :deep(ol) {
  padding-left: 1.25rem;
  margin: 0.5rem 0;
  color: var(--mobile-text-secondary);
}

.viewer-md-preview :deep(li) {
  margin: 0.25rem 0;
}

.viewer-md-preview :deep(hr) {
  border: none;
  border-top: 1px solid var(--mobile-border);
  margin: 1rem 0;
}

.viewer-md-preview :deep(a) {
  color: var(--mobile-accent);
  text-decoration: none;
}

.viewer-md-preview :deep(a:hover) {
  text-decoration: underline;
}

.viewer-md-preview :deep(img) {
  max-width: 100%;
  border-radius: 0.5rem;
}

.viewer-md-preview :deep(strong) {
  color: var(--mobile-text-primary);
  font-weight: 600;
}

.viewer-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 1rem;
  border-top: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

.viewer-lang-label {
  font-size: var(--font-size-sm);
  color: var(--mobile-text-muted);
  text-transform: capitalize;
}

.viewer-line-count {
  font-size: var(--font-size-sm);
  color: var(--mobile-text-muted);
}
</style>
