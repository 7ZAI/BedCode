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
        <slot name="header-right"></slot>
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

        <!-- 代码内容 -->
        <div
          v-else-if="highlightedHtml"
          class="code-content"
          :class="{ 'hide-line-numbers': !codeViewerStore.settings.showLineNumbers }"
          :style="codeStyle"
          v-html="highlightedHtml"
        ></div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * FileExplorer - 文件浏览 + 代码查看组件
 *
 * 封装文件树侧边栏 + 代码查看器（语法高亮）+ 长按复制 + 侧边栏切换
 * 供 CodeExplorerView（全屏）、ToolboxView（弹窗）等页面复用
 *
 * 内置 Header 布局：左侧按钮 + 文件名/标题 + 语言badge + 行数 + 侧边栏切换
 * - #header-left: 左侧按钮区域（默认关闭按钮，可替换为返回按钮等）
 * - #header-right: 右侧额外按钮区域（设置按钮、目录下拉等）
 *
 * mode:
 * - "standalone" - FileSidebar 内部处理文件查看，不显示代码区
 * - "emit" - FileSidebar 发出 fileSelect 事件，本组件显示代码区
 */

import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import FileSidebar from '@/components/FileSidebar.vue'
import { useCodeHighlight, getLangByFilename } from '@/composables/useCodeHighlight'
import { useHttpApi } from '@/composables/useHttpApi'
import { useToast } from '@/composables/useToast'
import { useOrientation } from '@/composables/useOrientation'
import { writeClipboardText } from '@/utils/clipboard'
import { useCodeViewerStore, resolveCodeTheme, CODE_THEMES } from '@/stores/codeViewer'
import { useTheme } from '@/composables/useTheme'

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
const codeViewerStore = useCodeViewerStore()
const { highlightedHtml, highlight, highlightDiff } = useCodeHighlight()

const resolvedTheme = computed(() => resolveCodeTheme(codeViewerStore.settings.theme, isSystemDark.value))

const codeBgColor = computed(() => {
  const themeConfig = CODE_THEMES[resolvedTheme.value]
  return themeConfig?.background ?? 'var(--mobile-bg-secondary)'
})

// ==================== Sidebar Toggle ====================

const sidebarVisible = ref(props.defaultShowSidebar)

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
  line-height: 0.8;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  tab-size: var(--code-tab-size, 4);
  background: var(--code-bg, var(--mobile-bg-secondary));
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
  padding-left: 3.5em;
  white-space: pre;
}

.code-content :deep(.line::before) {
  content: attr(data-line);
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3.2em;
  padding-right: 0.8em;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  color: var(--mobile-code-gutter-color);
  font-size: 0.85em;
  user-select: none;
  pointer-events: none;
  background: var(--mobile-code-gutter-bg);
  border-right: 1px solid var(--mobile-code-gutter-border);
}

.code-content :deep(.line:empty::after) {
  content: '\00a0';
}

/* 行号隐藏 */
.code-content.hide-line-numbers :deep(.line) {
  padding-left: 0.5em;
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
  min-height: 1.4em;
  line-height: 1.4;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  font-size: var(--code-font-size, 13px);
  white-space: pre;
  background: var(--code-bg, var(--mobile-bg-secondary));
}

.code-content :deep(.diff-line-no) {
  width: 3.2em;
  padding: 0 0.5em;
  text-align: right;
  font-size: 0.85em;
  user-select: none;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  background: var(--mobile-code-gutter-bg);
  border-right: 1px solid var(--mobile-code-gutter-border);
}

.code-content :deep(.diff-old-no) {
  color: rgba(220, 38, 38, 0.6);
}

.code-content :deep(.diff-new-no) {
  color: rgba(5, 150, 105, 0.6);
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
