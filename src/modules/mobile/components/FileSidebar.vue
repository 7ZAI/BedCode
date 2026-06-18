<template>
  <div class="file-sidebar" :style="sidebarStyle">
    <!-- 工具栏 -->
    <div class="sidebar-header">
      <span class="sidebar-title">文件</span>
      <div class="sidebar-actions">
        <button class="action-btn" title="刷新" @click="handleRefresh">
          <svg
            class="refresh-icon"
            :class="{ spinning: isRefreshing }"
            width="16"
            height="16"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
        <button class="action-btn" title="全部折叠" @click="collapseAll">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
          </svg>
        </button>
        <button class="action-btn" title="全部展开" @click="expandAll">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
          </svg>
        </button>
        <button class="action-btn" title="Diff" @click="handleDiff">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7h8m-8 5h8m-4-9v14M4 4h16a1 1 0 011 1v14a1 1 0 01-1 1H4a1 1 0 01-1-1V5a1 1 0 011-1z" />
          </svg>
        </button>
        <button class="action-btn" title="设置" @click="toggleSettings">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 设置面板 -->
    <transition name="dropdown">
      <div v-if="showSettingsPanel" class="settings-panel" @click.stop>
        <div class="settings-panel-section">
          <div class="settings-panel-row">
            <span class="settings-panel-label">默认展开</span>
            <button
              class="toggle-switch"
              :class="{ active: tempDefaultExpanded }"
              @click="tempDefaultExpanded = !tempDefaultExpanded"
            >
              <span class="toggle-knob"></span>
            </button>
          </div>
        </div>
        <div class="settings-panel-section">
          <label class="settings-panel-label">过滤目录</label>
          <input
            v-model="tempFilterText"
            class="settings-panel-input"
            placeholder="node_modules, target, .git"
          />
        </div>
        <div class="settings-panel-actions">
          <button class="settings-panel-btn cancel" @click="cancelSettingsPanel">取消</button>
          <button class="settings-panel-btn confirm" @click="confirmSettingsPanel">确认</button>
        </div>
      </div>
    </transition>

    <!-- 遮罩层（设置面板打开时） -->
    <div v-if="showSettingsPanel" class="settings-backdrop" @click="cancelSettingsPanel"></div>

    <!-- 文件树 -->
    <div class="sidebar-body">
      <!-- 加载状态 -->
      <div v-if="loading" class="sidebar-state">
        <svg class="spinning-icon" width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        <span class="state-text">加载中...</span>
      </div>

      <!-- 错误状态 -->
      <div v-else-if="error" class="sidebar-state error-state">
        <span class="state-text">{{ error }}</span>
        <button class="retry-btn" @click="handleRefresh">重试</button>
      </div>

      <!-- 空状态 -->
      <div v-else-if="tree.length === 0" class="sidebar-state">
        <span class="state-text">暂无文件</span>
      </div>

      <!-- 文件树列表 -->
      <template v-else>
        <FileTreeItem
          v-for="(node, index) in tree"
          :key="index"
          :node="node"
          :depth="0"
          @file-click="handleFileClick"
        />
      </template>
    </div>

    <!-- 文件查看弹窗 -->
    <FileViewerModal
      :visible="showFileViewer"
      :filename="selectedFile"
      @update:visible="showFileViewer = $event"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, toRef } from 'vue'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import { useFileTree, type SidebarSettings } from '@/modules/mobile/composables/useFileTree'
import FileTreeItem from './FileTreeItem.vue'
import FileViewerModal from './FileViewerModal.vue'
import { useToast } from '@/modules/shared/composables/useToast'

const props = defineProps<{
  sessionId: string
}>()

const { isLandscape } = useOrientation()
const toast = useToast()
const { tree, loading, error, expandAll, collapseAll, refresh, settings, updateSettings } = useFileTree(toRef(props, 'sessionId'))

const isRefreshing = ref(false)
const showSettingsPanel = ref(false)
const showFileViewer = ref(false)
const selectedFile = ref('')

// 临时设置状态
const tempDefaultExpanded = ref(false)
const tempFilterText = ref('')

const sidebarStyle = computed(() => {
  const widthPercent = isLandscape.value ? '30%' : '40%'
  return { width: widthPercent }
})

async function handleRefresh() {
  isRefreshing.value = true
  await refresh()
  if (error.value) {
    toast.error(error.value)
  }
  setTimeout(() => {
    isRefreshing.value = false
  }, 500)
}

function handleDiff() {
  // TODO: 实现 diff 功能
}

function toggleSettings() {
  if (showSettingsPanel.value) {
    cancelSettingsPanel()
  } else {
    // 用当前设置初始化临时状态
    tempDefaultExpanded.value = settings.value.defaultExpanded
    tempFilterText.value = settings.value.filterPatterns.join(', ')
    showSettingsPanel.value = true
  }
}

function cancelSettingsPanel() {
  showSettingsPanel.value = false
}

function confirmSettingsPanel() {
  const newSettings: SidebarSettings = {
    defaultExpanded: tempDefaultExpanded.value,
    filterPatterns: tempFilterText.value
      .split(',')
      .map(s => s.trim())
      .filter(Boolean),
  }
  updateSettings(newSettings)
  showSettingsPanel.value = false
}

function handleFileClick(name: string) {
  selectedFile.value = name
  showFileViewer.value = true
}
</script>

<style scoped>
.file-sidebar {
  display: flex;
  flex-direction: column;
  background: var(--mobile-bg-secondary);
  border-left: 1px solid var(--mobile-border);
  flex-shrink: 0;
  overflow: hidden;
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

.sidebar-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--mobile-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.sidebar-actions {
  display: flex;
  gap: 0.25rem;
}

.action-btn {
  padding: 0.25rem;
  border-radius: 0.25rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease, background-color 0.2s ease;
}

.action-btn:hover {
  color: var(--mobile-text-primary);
  background: var(--mobile-bg-elevated);
}

.action-btn:active {
  color: var(--mobile-accent);
}

.refresh-icon {
  transition: transform 0.5s ease;
}

.refresh-icon.spinning {
  transform: rotate(360deg);
}

/* Settings Panel */
.settings-panel {
  position: absolute;
  top: 2.5rem;
  left: 0.5rem;
  right: 0.5rem;
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  padding: 0.75rem;
  z-index: 30;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}

.settings-panel-section {
  margin-bottom: 0.75rem;
}

.settings-panel-section:last-of-type {
  margin-bottom: 0.5rem;
}

.settings-panel-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.settings-panel-label {
  font-size: 0.8125rem;
  color: var(--mobile-text-secondary);
}

.toggle-switch {
  width: 36px;
  height: 20px;
  border-radius: 10px;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  cursor: pointer;
  position: relative;
  transition: all 0.2s ease;
  padding: 0;
}

.toggle-switch.active {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
}

.toggle-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: white;
  transition: transform 0.2s ease;
}

.toggle-switch.active .toggle-knob {
  transform: translateX(16px);
}

.settings-panel-input {
  width: 100%;
  padding: 0.5rem 0.625rem;
  margin-top: 0.375rem;
  border-radius: 0.375rem;
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-primary);
  font-size: 0.8125rem;
  outline: none;
  transition: border-color 0.2s ease;
}

.settings-panel-input:focus {
  border-color: var(--mobile-accent);
}

.settings-panel-input::placeholder {
  color: var(--mobile-text-disabled);
}

.settings-panel-actions {
  display: flex;
  gap: 0.5rem;
}

.settings-panel-btn {
  flex: 1;
  padding: 0.5rem;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-panel-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.settings-panel-btn.cancel:hover {
  background: var(--mobile-bg-tertiary);
  color: var(--mobile-text-primary);
}

.settings-panel-btn.confirm {
  background: var(--mobile-accent);
  border: none;
  color: #0a0a0f;
}

.settings-panel-btn.confirm:hover {
  opacity: 0.9;
}

.settings-backdrop {
  position: absolute;
  inset: 0;
  z-index: 25;
}

/* Dropdown transition */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: all 0.2s ease;
}

.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

.sidebar-body {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  -webkit-overflow-scrolling: touch;
  padding: 0.25rem 0;

  /* Firefox */
  scrollbar-width: thin;
  scrollbar-color: rgba(100, 100, 120, 0.3) transparent;
}

/* Webkit scrollbar */
.sidebar-body::-webkit-scrollbar {
  width: 4px;
}

.sidebar-body::-webkit-scrollbar-track {
  background: transparent;
}

.sidebar-body::-webkit-scrollbar-thumb {
  background: rgba(100, 100, 120, 0.3);
  border-radius: 2px;
}

.sidebar-body::-webkit-scrollbar-thumb:hover {
  background: rgba(0, 212, 255, 0.4);
}

/* Sidebar States */
.sidebar-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem 1rem;
  gap: 0.5rem;
}

.state-text {
  font-size: 0.8125rem;
  color: var(--mobile-text-muted);
}

.spinning-icon {
  animation: spin 1s linear infinite;
  color: var(--mobile-text-muted);
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.error-state .state-text {
  color: var(--error, #ef4444);
}

.retry-btn {
  padding: 0.375rem 1rem;
  border-radius: 0.375rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  font-size: 0.8125rem;
  cursor: pointer;
  transition: all 0.2s ease;
}

.retry-btn:hover {
  border-color: var(--mobile-accent);
  color: var(--mobile-accent);
}
</style>
