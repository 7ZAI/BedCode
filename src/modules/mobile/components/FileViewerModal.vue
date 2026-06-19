<template>
  <transition name="modal-fade">
    <div v-if="visible" class="viewer-overlay" @click.self="handleClose" @touchstart.stop @touchmove.stop>
      <div class="viewer-modal" :class="{ 'viewer-fullscreen': isFullscreen }" :style="modalStyle">
        <!-- Header -->
        <div class="viewer-header">
          <div class="viewer-title-area">
            <span class="viewer-filename">{{ filename }}</span>
            <span class="viewer-lang-badge">{{ displayLang }}</span>
          </div>
          <div class="viewer-actions">
            <button class="viewer-action-btn" :title="isFullscreen ? '退出全屏' : '全屏'" @click="toggleFullscreen">
              <svg v-if="!isFullscreen" width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
              </svg>
              <svg v-else width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25" />
              </svg>
            </button>
            <button class="viewer-action-btn" title="关闭" @click="handleClose">
              <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Code Area -->
        <div class="viewer-body">
          <div v-if="loading" class="viewer-loading">加载中...</div>
          <div v-else-if="error" class="viewer-error">{{ error }}</div>
          <div v-else-if="!code" class="viewer-loading">选择文件查看内容</div>
          <div v-else class="viewer-code" v-html="highlightedHtml"></div>
        </div>

        <!-- Footer -->
        <div class="viewer-footer">
          <span class="viewer-lang-label">{{ displayLang }}</span>
          <span class="viewer-line-count">{{ lineCount }} 行</span>
        </div>
      </div>
    </div>
  </transition>
</template>

<script setup lang="ts">
import { ref, computed, watch, inject, type Ref } from 'vue'
import { useCodeHighlight, getLangByFilename } from '@/modules/mobile/composables/useCodeHighlight'

const props = defineProps<{
  visible: boolean
  filename: string
  code?: string
  loading?: boolean
  error?: string | null
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
}>()

const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!
const { highlightedHtml, isLoading, error, highlight } = useCodeHighlight()

const isFullscreen = ref(false)

const displayLang = computed(() => getLangByFilename(props.filename))

const lineCount = computed(() => {
  const content = props.code ?? ''
  if (!content) return 0
  return content.split('\n').length
})

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

// 当文件变化时重新高亮
watch(
  () => [props.visible, props.filename, props.code] as const,
  async ([visible, filename, code]) => {
    if (visible && filename && code) {
      const lang = getLangByFilename(filename)
      await highlight(code, lang)
    }
  },
  { immediate: true },
)
</script>

<style scoped>
.viewer-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 1rem;
}

.viewer-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 90%;
  max-width: 700px;
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
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.viewer-lang-badge {
  font-size: 0.625rem;
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
  padding: 0.375rem;
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
  scrollbar-color: rgba(100, 100, 120, 0.3) transparent;
}

.viewer-body::-webkit-scrollbar {
  width: 4px;
  height: 4px;
}

.viewer-body::-webkit-scrollbar-track {
  background: transparent;
}

.viewer-body::-webkit-scrollbar-thumb {
  background: rgba(100, 100, 120, 0.3);
  border-radius: 2px;
}

/* ==================== VS Code 风格代码区域 ==================== */

.viewer-code {
  margin: 0;
  padding: 0;
  font-size: 13px;
  line-height: 0.8;
  font-family: 'Fira Code', 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
  tab-size: 4;
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
  padding-left: 3.5em;
  white-space: pre;
}

/* 行号区 (gutter) — VS Code 风格：独立背景 + 右侧分隔线 */
.viewer-code :deep(.line::before) {
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

/* 空行保持行高 */
.viewer-code :deep(.line:empty::after) {
  content: '\00a0';
}

.viewer-loading,
.viewer-error {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--mobile-text-muted);
  font-size: 0.875rem;
}

.viewer-error {
  color: var(--mobile-error);
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
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
  text-transform: capitalize;
}

.viewer-line-count {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

/* Modal transition */
.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 0.2s ease;
}

.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}

.modal-fade-enter-active .viewer-modal,
.modal-fade-leave-active .viewer-modal {
  transition: transform 0.2s ease;
}

.modal-fade-enter-from .viewer-modal,
.modal-fade-leave-to .viewer-modal {
  transform: scale(0.95);
}
</style>
