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
        <span v-if="selectedFile" class="header-line-count">{{ lineCount }} 行</span>
      </div>
    </header>

    <!-- Main: Sidebar + Code Area -->
    <div class="explorer-body">
      <!-- 左侧文件树侧边栏 -->
      <FileSidebar
        class="explorer-sidebar"
        :session-id="sessionId"
        mode="emit"
        @file-select="handleFileSelect"
      />

      <!-- 右侧代码显示区 -->
      <div class="explorer-code-area">
        <!-- 未选择文件 -->
        <div v-if="!selectedFile" class="code-empty">
          <svg class="w-8 h-8 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
          </svg>
          <p class="text-[var(--mobile-text-disabled)] text-sm mt-2">选择文件查看内容</p>
        </div>

        <!-- 加载中 -->
        <div v-else-if="fileLoading" class="code-state">
          <svg class="w-5 h-5 animate-spin text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
          <span class="text-[var(--mobile-text-muted)] text-sm">加载中...</span>
        </div>

        <!-- 错误 -->
        <div v-else-if="fileError" class="code-state">
          <p class="text-red-400 text-sm">{{ fileError }}</p>
          <button class="text-xs text-[var(--mobile-accent)] mt-2" @click="retryLoadFile">重试</button>
        </div>

        <!-- 代码内容 -->
        <div v-else-if="highlightedHtml" class="code-content" v-html="highlightedHtml"></div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * CodeExplorerView - 代码查看页面
 *
 * 左侧边栏文件树 + 右侧代码显示区
 * 复用 FileSidebar (emit 模式) + useCodeHighlight
 */

import { ref, computed, inject, type Ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import { useCodeHighlight, getLangByFilename } from '@/modules/mobile/composables/useCodeHighlight'
import { useHttpApi } from '@/modules/mobile/composables/useHttpApi'
import FileSidebar from '@/modules/mobile/components/FileSidebar.vue'

const router = useRouter()
const route = useRoute()
const connection = useMobileConnection()
const { highlightedHtml, highlight, highlightDiff } = useCodeHighlight()
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

const sessionId = computed(() => route.params.id as string)

// ==================== Config Info ====================

const configName = computed(() => {
  const session = connection.activeSessions.value.find(
    (s: any) => s.id === sessionId.value
  )
  if (!session) return '代码查看'
  const configId = session.config_id || session.configId
  const config = connection.sessionConfigs.value.find(c => c.id === configId)
  return config?.name || '代码查看'
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
      throw new Error(result.message || '获取文件内容失败')
    }
    fileContent.value = result.data.content

    const lang = getLangByFilename(selectedFile.value)
    await highlight(result.data.content, lang)
  } catch (e: any) {
    fileError.value = e?.toString() || '获取文件内容失败'
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
      throw new Error(result.message || '获取文件 Diff 失败')
    }

    const lang = getLangByFilename(selectedFile.value)
    await highlightDiff(result.data.lines, lang)
  } catch (e: any) {
    fileError.value = e?.toString() || '获取文件 Diff 失败'
  } finally {
    fileLoading.value = false
  }
}

async function retryLoadFile() {
  if (selectedFilePath.value) {
    await loadFileContent(selectedFilePath.value)
  }
}

// ==================== Navigation ====================

function handleBack() {
  router.back()
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
  font-size: 13px;
  line-height: 0.8;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  tab-size: 4;
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
  color: rgba(100, 100, 120, 0.45);
  font-size: 0.85em;
  user-select: none;
  pointer-events: none;
  background: rgba(0, 0, 0, 0.18);
  border-right: 1px solid rgba(100, 100, 120, 0.12);
}

.code-content :deep(.line:empty::after) {
  content: '\00a0';
}

/* ==================== Diff 行样式 ==================== */

.code-content :deep(.diff-line) {
  display: flex;
  align-items: stretch;
  min-height: 1.4em;
  line-height: 1.4;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  font-size: 13px;
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
  background: rgba(0, 0, 0, 0.18);
  border-right: 1px solid rgba(100, 100, 120, 0.12);
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
  color: rgba(100, 100, 120, 0.45);
}
</style>
