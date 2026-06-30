<template>
  <div class="code-explorer" :style="explorerStyle">
    <!-- Header -->
    <header class="explorer-header">
      <button class="back-btn" @click="handleBack">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="header-info">
        <span class="header-filename">{{ selectedFile || configName }}</span>
        <span v-if="selectedFile" class="header-lang-badge">{{ displayLang }}</span>
      </div>
      <div class="header-meta">
        <button class="settings-btn" @click="showSettings = true" :title="t('mobile.codeViewer.settingsTitle')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
        <button class="sidebar-toggle-btn" :class="{ active: showSidebar }" @click="showSidebar = !showSidebar" :title="t('mobile.file.title')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
        </button>
        <span v-if="selectedFile" class="header-line-count">{{ t('common.misc.lineCount', { count: lineCount }) }}</span>
      </div>
    </header>

    <!-- Main: Sidebar + Code Area -->
    <div class="explorer-body">
      <!-- 左侧文件树侧边栏 -->
      <transition name="sidebar-slide">
        <FileSidebar
          v-if="showSidebar"
          class="explorer-sidebar"
          :session-id="sessionId"
          mode="emit"
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

    <!-- Settings Modal -->
    <CodeViewerSettingsModal
      :visible="showSettings"
      @close="showSettings = false"
      @confirm="onSettingsConfirm"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * CodeExplorerView - 代码查看页面
 *
 * 左侧边栏文件树 + 右侧代码显示区
 * 复用 FileSidebar (emit 模式) + useCodeHighlight
 */

import { ref, computed, watch, inject, type Ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { useCodeHighlight, getLangByFilename } from '@/modules/mobile/composables/useCodeHighlight'
import { useHttpApi } from '@/modules/mobile/composables/useHttpApi'
import FileSidebar from '@/modules/mobile/components/FileSidebar.vue'
import { useToast } from '@/modules/shared/composables/useToast'
import { writeClipboardText } from '@/modules/shared/utils/clipboard'
import { useCodeViewerStore } from '@/modules/shared/stores/codeViewer'
import CodeViewerSettingsModal from '@/modules/mobile/components/CodeViewerSettingsModal.vue'

const router = useRouter()
const route = useRoute()
const connection = useMobileConnection()
const toast = useToast()
const { t } = useI18n()
const codeViewerStore = useCodeViewerStore()
const showSettings = ref(false)
const { highlightedHtml, highlight, highlightDiff } = useCodeHighlight()
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

const sessionId = computed(() => route.params.id as string)
const showSidebar = ref(true)

// ==================== Config Info ====================

const configName = computed(() => {
  const session = connection.activeSessions.value.find(
    (s: any) => s.id === sessionId.value
  )
  if (!session) return t('mobile.codeViewer.title')
  const configId = session.config_id || session.configId
  const config = connection.sessionConfigs.value.find(c => c.id === configId)
  return config?.name || t('mobile.codeViewer.title')
})

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

// ==================== Layout ====================

const explorerStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
}))

// ==================== File Operations ====================

async function handleFileSelect(name: string, path: string, isDiff: boolean) {
  selectedFile.value = name
  selectedFilePath.value = path
  if (isDiff) {
    await loadFileDiff(path)
  } else {
    await loadFileContent(path)
  }
}

async function loadFileContent(path: string) {
  fileLoading.value = true
  fileError.value = null
  fileContent.value = ''

  try {
    const { httpGetFileContent } = useHttpApi()
    const result = await httpGetFileContent(sessionId.value, path)
    if (result.code !== 0 || !result.data) {
      throw new Error(result.message || 'mobile.codeViewer.fetchContentFailed')
    }
    fileContent.value = result.data.content

    const lang = getLangByFilename(selectedFile.value)
    await highlight(result.data.content, lang, codeViewerStore.settings.theme)
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
    const result = await httpGetFileDiff(sessionId.value, path)
    if (result.code !== 0 || !result.data) {
      throw new Error(result.message || 'mobile.codeViewer.fetchDiffFailed')
    }

    const lang = getLangByFilename(selectedFile.value)
    await highlightDiff(result.data.lines, lang, codeViewerStore.settings.theme)
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

// ==================== Long Press Copy Path ====================

async function handleLongPress(name: string, path: string) {
  try {
    await writeClipboardText(path)
    toast.success(t('mobile.codeViewer.copied', { path }))
  } catch {
    toast.error(t('mobile.codeViewer.copyFailed'))
  }
}

// ==================== Code Style ====================

const codeStyle = computed(() => ({
  '--code-font-size': `${codeViewerStore.settings.fontSize}px`,
  '--code-tab-size': codeViewerStore.settings.tabSize,
}))

// 监听主题变化，重新高亮代码
watch(
  () => codeViewerStore.settings.theme,
  () => {
    if (selectedFile.value && fileContent.value) {
      const lang = getLangByFilename(selectedFile.value)
      highlight(fileContent.value, lang, codeViewerStore.settings.theme)
    }
  },
)

// ==================== Navigation ====================

function handleBack() {
  router.back()
}

async function onSettingsConfirm() {
  // 主题变化时重新高亮（watch 已处理）
  // 字体大小、tab 缩进、行号通过 CSS 变量实时生效，无需额外操作
}
</script>

<style scoped>
.code-explorer {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--mobile-bg-primary);
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
  overflow: hidden;
}

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

.back-btn {
  padding: 0.375rem;
  margin-left: -0.375rem;
  color: var(--mobile-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.back-btn:active {
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

.sidebar-toggle-btn {
  padding: 0.375rem;
  border-radius: 0.375rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease;
}

.sidebar-toggle-btn.active {
  color: var(--mobile-accent);
}

.header-line-count {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

/* ==================== Body Layout ==================== */

.explorer-body {
  flex: 1;
  display: flex;
  overflow: hidden;
}

/* 左侧边栏：竖屏 35%，横屏 25% */
.explorer-sidebar {
  width: 35%;
  border-right: 1px solid var(--mobile-border);
  border-left: none;
  flex-shrink: 0;
}

@media (orientation: landscape) {
  .explorer-sidebar {
    width: 25%;
  }
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
  padding: 0;
  font-size: var(--code-font-size, 13px);
  line-height: 0.8;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  tab-size: var(--code-tab-size, 4);
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
  color: rgba(248, 81, 73, 0.6);
}

.code-content :deep(.diff-new-no) {
  color: rgba(63, 185, 80, 0.6);
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
  color: rgba(248, 81, 73, 0.8);
}
.code-content :deep(.diff-removed .diff-new-no) {
  background: rgba(248, 81, 73, 0.08);
}

.code-content :deep(.diff-added) {
  background: rgba(63, 185, 80, 0.15);
}
.code-content :deep(.diff-added .diff-marker) {
  color: rgba(63, 185, 80, 0.8);
}
.code-content :deep(.diff-added .diff-old-no) {
  background: rgba(63, 185, 80, 0.08);
}

.code-content :deep(.diff-context .diff-line-no) {
  color: var(--mobile-code-gutter-color);
}

/* ==================== Settings Button ==================== */

.settings-btn {
  padding: 0.375rem;
  border-radius: 0.375rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease;
}

.settings-btn:active {
  color: var(--mobile-accent);
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
