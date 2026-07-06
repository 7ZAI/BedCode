<template>
  <div class="file-explorer flex flex-col h-full bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="explorer-header">
      <slot name="header-left">
        <button class="header-btn" @click="$emit('close')">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </slot>
      <div class="header-info">
        <span class="header-filename">{{ selectedFile || title }}</span>
        <span v-if="selectedFile && displayLang" class="header-lang-badge">{{ displayLang }}</span>
      </div>
      <div class="header-meta">
        <span v-if="selectedFile && lineCount" class="header-line-count">{{ t('common.misc.lineCount', { count: lineCount }) }}</span>
        <!-- Markdown 预览/源码切换（仅 .md 文件显示） -->
        <button v-if="isMarkdownFile" class="header-btn" :class="{ 'header-btn--active': viewMode === 'preview' }" :title="t('mobile.file.previewMode')" @click="viewMode = 'preview'">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
          </svg>
        </button>
        <button v-if="isMarkdownFile" class="header-btn" :class="{ 'header-btn--active': viewMode === 'source' }" :title="t('mobile.file.sourceMode')" @click="viewMode = 'source'">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
          </svg>
        </button>
        <slot name="header-right"></slot>
        <button v-if="!hasHeaderRightSlot" class="header-btn" :title="t('mobile.codeViewer.settingsTitle')" @click="showCodeViewerSettings = true">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
        <button class="header-btn" :class="{ 'header-btn--active': sidebarVisible }" @click="toggleSidebar" :title="t('mobile.file.title')">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Body: 文件树 + 代码查看区 -->
    <div class="flex-1 flex overflow-hidden">
      <!-- 左侧文件树侧边栏 -->
      <transition name="sidebar-slide">
        <FileSidebar
          v-if="sidebarVisible && sessionId"
          class="explorer-sidebar"
          :class="{ 'landscape-sidebar': isLandscape }"
          :session-id="sessionId"
          :mode="mode"
          resize-side="right"
          @file-select="handleFileSelect"
          @long-press="handleLongPress"
        />
      </transition>

      <!-- 右侧代码显示区 -->
      <div class="explorer-code-area">
        <!-- 未选择文件 -->
        <div v-if="!selectedFile" class="code-empty">
          <svg class="w-8 h-8 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
          </svg>
          <p class="text-[var(--mobile-text-disabled)] text-sm mt-2">{{ t('mobile.codeViewer.selectFile') }}</p>
        </div>

        <!-- 加载中 -->
        <div v-else-if="fileLoading" class="code-state">
          <svg class="w-5 h-5 animate-spin text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
          <span class="text-[var(--mobile-text-muted)] text-sm">{{ t('mobile.codeViewer.loading') }}</span>
        </div>

        <!-- 错误 -->
        <div v-else-if="fileError" class="code-state">
          <p class="text-red-400 text-sm">{{ fileError }}</p>
          <button class="text-xs text-[var(--mobile-accent)] mt-2" @click="retryLoadFile">{{ t('mobile.codeViewer.retry') }}</button>
        </div>

        <!-- Markdown 预览模式 -->
        <div
          v-else-if="isMarkdownFile && viewMode === 'preview'"
          class="code-md-preview"
          v-html="renderedMarkdown"
        ></div>

        <!-- 源码模式（shiki 高亮） -->
        <div
          v-else-if="highlightedHtml"
          class="code-content"
          :class="{ 'hide-line-numbers': !codeViewerStore.settings.showLineNumbers }"
          :style="codeStyle"
          v-html="highlightedHtml"
        ></div>
      </div>
    </div>

    <!-- Code Viewer Settings Modal (仅内部使用时渲染) -->
    <CodeViewerSettingsModal
      v-if="!hasHeaderRightSlot"
      :visible="showCodeViewerSettings"
      :z-index="130"
      @close="showCodeViewerSettings = false"
      @confirm="showCodeViewerSettings = false"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * FileExplorer - 文件浏览 + 代码查看组件
 *
 * 封装文件树侧边栏 + 代码查看器（语法高亮）+ Markdown 预览 + 长按复制 + 侧边栏切换
 * 供 CodeExplorerView（全屏）、ToolboxView（弹窗）等页面复用
 *
 * 内置 Header 布局：左侧按钮 + 文件名/标题 + 语言badge + 行数 + Markdown切换 + 侧边栏切换
 * - #header-left: 左侧按钮区域（默认关闭按钮，可替换为返回按钮等）
 * - #header-right: 右侧额外按钮区域（设置按钮、目录下拉等）
 *
 * mode:
 * - "standalone" - FileSidebar 内部处理文件查看，不显示代码区
 * - "emit" - FileSidebar 发出 fileSelect 事件，本组件显示代码区
 */

import { ref, computed, watch, useSlots } from 'vue'
import { useI18n } from 'vue-i18n'
import { marked } from 'marked'
import FileSidebar from '@/components/FileSidebar.vue'
import { useCodeHighlight, getLangByFilename } from '@/composables/useCodeHighlight'
import { useHttpApi } from '@/composables/useHttpApi'
import { useToast } from '@/composables/useToast'
import { useOrientation } from '@/composables/useOrientation'
import { writeClipboardText } from '@/utils/clipboard'
import { useCodeViewerStore, resolveCodeTheme, CODE_THEMES } from '@/stores/codeViewer'
import { useTheme } from '@/composables/useTheme'
import CodeViewerSettingsModal from '@/components/CodeViewerSettingsModal.vue'

const props = withDefaults(defineProps<{
  sessionId: string
  /** FileSidebar 模式，默认 standalone */
  mode?: 'standalone' | 'emit'
  /** 是否默认显示文件树侧边栏，默认 true */
  defaultShowSidebar?: boolean
  /** 未选文件时 header 显示的标题 */
  title?: string
}>(), {
  mode: 'standalone',
  defaultShowSidebar: true,
  title: '',
})

const emit = defineEmits<{
  fileSelect: [name: string, path: string, isDiff: boolean]
  longPress: [name: string, path: string]
  close: []
}>()

const { t } = useI18n()
const toast = useToast()
const { isLandscape } = useOrientation()
const { isSystemDark } = useTheme()
const slots = useSlots()
const codeViewerStore = useCodeViewerStore()
const { highlightedHtml, highlight, highlightDiff } = useCodeHighlight()

/** 外部已通过 header-right slot 传入设置按钮时，不显示内置按钮 */
const hasHeaderRightSlot = computed(() => !!slots['header-right'])

const resolvedTheme = computed(() => resolveCodeTheme(codeViewerStore.settings.theme, isSystemDark.value))

const codeBgColor = computed(() => {
  const themeConfig = CODE_THEMES[resolvedTheme.value]
  return themeConfig?.background ?? 'var(--mobile-bg-secondary)'
})

// ==================== Markdown Preview ====================

/** Markdown 文件预览/源码切换：默认预览 */
const viewMode = ref<'preview' | 'source'>('preview')

const isMarkdownFile = computed(() => {
  const ext = selectedFile.value.split('.').pop()?.toLowerCase() || ''
  return ext === 'md' || ext === 'mdx'
})

const renderedMarkdown = computed(() => {
  if (!fileContent.value || !isMarkdownFile.value) return ''
  return marked.parse(fileContent.value) as string
})

// ==================== Sidebar Toggle ====================

const sidebarVisible = ref(props.defaultShowSidebar)
const showCodeViewerSettings = ref(false)

function toggleSidebar() {
  sidebarVisible.value = !sidebarVisible.value
}

// ==================== File State ====================

const selectedFile = ref('')
const selectedFilePath = ref('')
const fileContent = ref('')
const fileLoading = ref(false)
const fileError = ref<string | null>(null)

const displayLang = computed(() => getLangByFilename(selectedFile.value))

const lineCount = computed(() => {
  if (!fileContent.value) return 0
  return fileContent.value.split('\n').length
})

// ==================== File Operations ====================

async function handleFileSelect(name: string, path: string, isDiff: boolean) {
  selectedFile.value = name
  selectedFilePath.value = path
  // 切换文件时重置为预览模式
  viewMode.value = 'preview'
  if (isDiff) {
    await loadFileDiff(path)
  } else {
    await loadFileContent(path)
  }
  emit('fileSelect', name, path, isDiff)
}

async function loadFileContent(path: string) {
  fileLoading.value = true
  fileError.value = null
  fileContent.value = ''

  try {
    const { httpGetFileContent } = useHttpApi()
    const result = await httpGetFileContent(props.sessionId, path)
    if (result.code !== 0 || !result.data) {
      throw new Error(result.message || 'mobile.codeViewer.fetchContentFailed')
    }
    fileContent.value = result.data.content

    const lang = getLangByFilename(selectedFile.value)
    await highlight(result.data.content, lang, resolvedTheme.value)
  } catch (e: any) {
    fileError.value = e?.toString() || 'mobile.codeViewer.fetchContentFailed'
  } finally {
    fileLoading.value = false
  }
}

async function loadFileDiff(path: string) {
  fileLoading.value = true
  fileError.value = null
  fileContent.value = ''

  try {
    const { httpGetFileDiff } = useHttpApi()
    const result = await httpGetFileDiff(props.sessionId, path)
    if (result.code !== 0 || !result.data) {
      throw new Error(result.message || 'mobile.codeViewer.fetchDiffFailed')
    }

    const lang = getLangByFilename(selectedFile.value)
    await highlightDiff(result.data.lines, lang, resolvedTheme.value)
  } catch (e: any) {
    fileError.value = e?.toString() || 'mobile.codeViewer.fetchDiffFailed'
  } finally {
    fileLoading.value = false
  }
}

async function retryLoadFile() {
  if (selectedFilePath.value) {
    await loadFileContent(selectedFilePath.value)
  }
}

async function handleLongPress(name: string, path: string) {
  try {
    await writeClipboardText(path)
    toast.success(t('mobile.codeViewer.copied', { path }))
  } catch {
    toast.error(t('mobile.codeViewer.copyFailed'))
  }
  emit('longPress', name, path)
}

// ==================== Code Style ====================

const codeStyle = computed(() => ({
  '--code-font-size': `${codeViewerStore.settings.fontSize}px`,
  '--code-line-height': codeViewerStore.settings.lineHeight,
  '--code-tab-size': codeViewerStore.settings.tabSize,
  '--code-bg': codeBgColor.value,
}))

// 监听主题设置或系统暗色模式变化，重新高亮代码
watch(
  [() => codeViewerStore.settings.theme, isSystemDark],
  () => {
    if (selectedFile.value && fileContent.value) {
      const lang = getLangByFilename(selectedFile.value)
      highlight(fileContent.value, lang, resolvedTheme.value)
    }
  },
)
</script>

<style scoped>
/* ==================== Header ==================== */

.explorer-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem 0.75rem;
  background: var(--mobile-bg-secondary);
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

.header-btn {
  padding: 0.375rem;
  margin-left: -0.375rem;
  color: var(--mobile-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease;
}

.header-btn:active {
  color: var(--mobile-accent);
}

.header-btn--active {
  color: var(--mobile-accent);
}

.header-info {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.header-filename {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.header-lang-badge {
  font-size: 0.625rem;
  font-weight: 600;
  color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
  text-transform: uppercase;
  flex-shrink: 0;
}

.header-meta {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.header-line-count {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

/* ==================== Sidebar Layout ==================== */

.explorer-sidebar {
  width: 35%;
  border-right: 1px solid var(--mobile-border);
  border-left: none;
  flex-shrink: 0;
}

.explorer-sidebar.landscape-sidebar {
  width: 25%;
}

/* ==================== Code Area ==================== */

.explorer-code-area {
  flex: 1;
  overflow: auto;
  -webkit-overflow-scrolling: touch;
  background: var(--mobile-bg-primary);

  scrollbar-width: thin;
  scrollbar-color: rgba(100, 100, 120, 0.3) transparent;
}

/* 代码查看时，滚动容器背景跟随代码主题色，避免横向滚动时右侧空白 */
.explorer-code-area:has(.code-content) {
  background: var(--code-bg, var(--mobile-bg-secondary));
}

.explorer-code-area::-webkit-scrollbar {
  width: 4px;
  height: 4px;
}

.explorer-code-area::-webkit-scrollbar-track {
  background: transparent;
}

.explorer-code-area::-webkit-scrollbar-thumb {
  background: rgba(100, 100, 120, 0.3);
  border-radius: 2px;
}

/* 空状态 / 加载 / 错误 */
.code-empty,
.code-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: 2rem;
}

/* ==================== VS Code 风格代码区 ==================== */

.code-content {
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

.code-content :deep(pre) {
  margin: 0;
  padding: 0;
  background: transparent !important;
}

.code-content :deep(code) {
  font-family: inherit;
  font-size: inherit;
  line-height: inherit;
  display: block;
  padding: 0;
}

.code-content :deep(.line) {
  display: block;
  position: relative;
  padding-left: 2.8em;
  white-space: pre;
}

.code-content :deep(.line::before) {
  content: attr(data-line);
  position: absolute;
  left: 0;
  top: 0;
  width: 2.8em;
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

.code-content :deep(.line:empty::after) {
  content: '\00a0';
}

/* 行号隐藏 */
.code-content.hide-line-numbers :deep(.line) {
  padding-left: 0.75em;
}

.code-content.hide-line-numbers :deep(.line::before) {
  content: none;
}

.code-content.hide-line-numbers :deep(.diff-line-no) {
  display: none;
}

/* ==================== Diff 行样式 ==================== */

.code-content :deep(.diff-line) {
  display: flex;
  align-items: stretch;
  line-height: 1.5;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  font-size: var(--code-font-size, 13px);
  white-space: pre;
  background: var(--code-bg, var(--mobile-bg-secondary));
}

.code-content :deep(.diff-line-no) {
  width: 2.8em;
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

.code-content :deep(.diff-old-no) {
  color: rgba(220, 38, 38, 0.6);
}

.code-content :deep(.diff-new-no) {
  color: rgba(5, 150, 105, 0.6);
  position: sticky;
  left: 2.8em;
}

.code-content :deep(.diff-marker) {
  width: 1.2em;
  text-align: center;
  font-size: 0.85em;
  user-select: none;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  position: sticky;
  left: 5.6em;
  z-index: 1;
}

.code-content :deep(.diff-content) {
  flex: 1;
  min-width: 0;
  padding-left: 0.5em;
}

.code-content :deep(.diff-removed) {
  background: rgba(248, 81, 73, 0.15);
}

.code-content :deep(.diff-removed .diff-marker) {
  color: rgba(220, 38, 38, 0.9);
}

.code-content :deep(.diff-removed .diff-new-no) {
  background: rgba(248, 81, 73, 0.08);
}

.code-content :deep(.diff-added) {
  background: rgba(63, 185, 80, 0.15);
}

.code-content :deep(.diff-added .diff-marker) {
  color: rgba(5, 150, 105, 0.9);
}

.code-content :deep(.diff-added .diff-old-no) {
  background: rgba(63, 185, 80, 0.08);
}

.code-content :deep(.diff-context .diff-line-no) {
  color: var(--mobile-code-gutter-color);
}

/* ==================== Markdown 预览 ==================== */

.code-md-preview {
  padding: 1rem;
  font-size: 0.875rem;
  line-height: 1.7;
  color: var(--mobile-text-primary);
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  word-break: break-word;
}

.code-md-preview :deep(h1) {
  font-size: 1.375rem;
  font-weight: 700;
  color: var(--mobile-text-primary);
  margin: 0 0 0.75rem;
  padding-bottom: 0.375rem;
  border-bottom: 1px solid var(--mobile-border);
}

.code-md-preview :deep(h2) {
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--mobile-accent);
  margin: 1.25rem 0 0.5rem;
  padding-bottom: 0.25rem;
  border-bottom: 1px solid var(--mobile-border);
}

.code-md-preview :deep(h3) {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 1rem 0 0.375rem;
}

.code-md-preview :deep(p) {
  margin: 0.5rem 0;
  color: var(--mobile-text-secondary);
}

.code-md-preview :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 0.5rem 0 1rem;
  font-size: 0.8125rem;
  display: block;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}

.code-md-preview :deep(thead th) {
  text-align: left;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-weight: 600;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-bottom: 2px solid var(--mobile-border);
  white-space: nowrap;
}

.code-md-preview :deep(tbody td) {
  padding: 0.4375rem 0.75rem;
  border-bottom: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

.code-md-preview :deep(tbody tr:last-child td) {
  border-bottom: none;
}

.code-md-preview :deep(code) {
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  font-size: 0.8125rem;
  padding: 0.125rem 0.375rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.25rem;
  color: var(--mobile-accent);
}

.code-md-preview :deep(pre) {
  margin: 0.75rem 0;
  padding: 0.75rem 1rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.5rem;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}

.code-md-preview :deep(pre code) {
  padding: 0;
  background: none;
  border: none;
  border-radius: 0;
  font-size: 0.8125rem;
  color: var(--mobile-text-primary);
}

.code-md-preview :deep(blockquote) {
  margin: 0.75rem 0;
  padding: 0.5rem 0.75rem;
  border-left: 3px solid var(--mobile-accent);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  border-radius: 0 0.375rem 0.375rem 0;
}

.code-md-preview :deep(ul),
.code-md-preview :deep(ol) {
  padding-left: 1.25rem;
  margin: 0.5rem 0;
  color: var(--mobile-text-secondary);
}

.code-md-preview :deep(li) {
  margin: 0.25rem 0;
}

.code-md-preview :deep(hr) {
  border: none;
  border-top: 1px solid var(--mobile-border);
  margin: 1rem 0;
}

.code-md-preview :deep(a) {
  color: var(--mobile-accent);
  text-decoration: none;
}

.code-md-preview :deep(a:hover) {
  text-decoration: underline;
}

.code-md-preview :deep(img) {
  max-width: 100%;
  border-radius: 0.5rem;
}

.code-md-preview :deep(strong) {
  color: var(--mobile-text-primary);
  font-weight: 600;
}

/* ==================== Sidebar Slide Transition ==================== */

.sidebar-slide-enter-active,
.sidebar-slide-leave-active {
  transition: width 0.25s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.2s ease;
  overflow: hidden;
}

.sidebar-slide-enter-from,
.sidebar-slide-leave-to {
  width: 0;
  opacity: 0;
}
</style>
