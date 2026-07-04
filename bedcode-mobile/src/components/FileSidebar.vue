<template>
  <div class="file-sidebar" :style="sidebarStyle" @touchstart.stop @touchmove.stop>
    <!-- 拖动调整宽度的手柄 -->
    <div
      class="resize-handle"
      :class="[`resize-handle--${resizeSide}`]"
      @pointerdown="onResizePointerDown"
    ></div>
    <!-- 工具栏 -->
    <div class="sidebar-header">
      <div class="branch-selector" @click.stop="showBranchDropdown = !showBranchDropdown">
        <template v-if="!isGitRepo">
          <span class="branch-label branch-label--no-git">{{ t('mobile.file.noGit') }}</span>
        </template>
        <template v-else-if="branchesLoading">
          <svg class="branch-spinner" width="12" height="12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </template>
        <template v-else>
          <svg class="branch-icon" width="14" height="14" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
          <span class="branch-label" :class="{ 'branch-label--switching': branchSwitching }">{{ currentBranch || t('mobile.file.branch') }}</span>
          <svg class="branch-chevron" :class="{ open: showBranchDropdown }" width="12" height="12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
          </svg>
        </template>

        <!-- 分支下拉列表 -->
        <transition name="dropdown">
          <div v-if="showBranchDropdown && branches.length > 0" class="branch-dropdown" @click.stop>
            <div class="branch-dropdown-title">{{ t('mobile.file.switchBranch') }}</div>
            <div class="branch-dropdown-list">
              <button
                v-for="b in branches"
                :key="b"
                class="branch-dropdown-item"
                :class="{ active: b === currentBranch }"
                :disabled="b === currentBranch || branchSwitching"
                @click="switchBranch(b)"
              >
                <span class="branch-dropdown-item-name">{{ b }}</span>
                <svg v-if="b === currentBranch" width="14" height="14" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                </svg>
              </button>
            </div>
          </div>
        </transition>
      </div>
      <div class="sidebar-actions">
        <button class="action-btn" :title="t('mobile.file.refresh')" @click="handleRefresh">
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
        <button class="action-btn" :title="t('mobile.file.collapseAll')" @click="collapseAll">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
          </svg>
        </button>
        <button class="action-btn" :title="t('mobile.file.expandAll')" @click="expandAll">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
          </svg>
        </button>
        <button class="action-btn" :class="{ active: isDiffMode }" title="Diff" @click="handleDiff">
          <svg width="16" height="16" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7h8m-8 5h8m-4-9v14M4 4h16a1 1 0 011 1v14a1 1 0 01-1 1H4a1 1 0 01-1-1V5a1 1 0 011-1z" />
          </svg>
        </button>
        <button class="action-btn" :title="t('mobile.file.settings')" @click="toggleSettings">
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
            <span class="settings-panel-label">{{ t('mobile.file.defaultExpand') }}</span>
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
          <div class="settings-panel-row">
            <span class="settings-panel-label">{{ t('mobile.file.treeSize') }}</span>
            <span class="font-size-value">{{ tempFontSize }}px</span>
          </div>
          <div
            ref="sliderTrackRef"
            class="slider-track"
            @pointerdown="onSliderPointerDown"
          >
            <div class="slider-fill" :style="sliderFillStyle"></div>
            <div class="slider-thumb" :style="sliderThumbStyle"></div>
            <div class="slider-dots">
              <span
                v-for="dot in sliderDots"
                :key="dot"
                class="slider-dot"
                :class="{ active: dot <= tempFontSize }"
              ></span>
            </div>
          </div>
          <div class="slider-range-labels">
            <span>{{ FONT_SIZE_MIN }}px</span>
            <span>{{ FONT_SIZE_MAX }}px</span>
          </div>
        </div>
        <div class="settings-panel-section">
          <label class="settings-panel-label">{{ t('mobile.file.filterDirs') }}</label>
          <input
            v-model="tempFilterText"
            class="settings-panel-input"
            placeholder="node_modules, target, .git"
          />
        </div>
        <div class="settings-panel-actions">
          <button class="settings-panel-btn cancel" @click="cancelSettingsPanel">{{ t('common.button.cancel') }}</button>
          <button class="settings-panel-btn confirm" @click="confirmSettingsPanel">{{ t('common.button.confirm') }}</button>
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
        <span class="state-text">{{ t('mobile.file.loading') }}</span>
      </div>

      <!-- 错误状态 -->
      <div v-else-if="error" class="sidebar-state error-state">
        <span class="state-text">{{ error }}</span>
        <button class="retry-btn" @click="handleRefresh">{{ t('mobile.file.retry') }}</button>
      </div>

      <!-- 空状态 -->
      <div v-else-if="tree.length === 0" class="sidebar-state">
        <span class="state-text">{{ isDiffMode ? t('mobile.file.noDiffFiles') : t('mobile.file.noFiles') }}</span>
      </div>

      <!-- 文件树列表 -->
      <template v-else>
        <FileTreeItem
          v-for="(node, index) in tree"
          :key="index"
          :node="node"
          :depth="0"
          :font-size="settings.fontSize"
          @file-click="handleFileClick"
          @long-press="handleLongPress"
        />
      </template>
    </div>

    <!-- 文件查看弹窗（仅 standalone 模式） -->
    <FileViewerModal
      v-if="mode === 'standalone'"
      :visible="showFileViewer"
      :filename="selectedFile"
      :code="fileContent"
      :diff-lines="diffLines"
      :loading="fileLoading"
      :error="fileError"
      @update:visible="showFileViewer = $event"
    />

    <!-- 分支切换确认弹窗 -->
    <Modal v-model="showBranchConfirm" :title="t('mobile.file.switchConfirmTitle')" size="sm">
      <p class="text-[var(--mobile-text-disabled)] text-sm">{{ branchConfirmMsg }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showBranchConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="branchSwitching" @click="confirmSwitchBranch">{{ t('common.button.confirm') }}</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, toRef, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHttpApi, type GitBranchesData, type GitStatusData, type FileDiffLine } from '../composables/useHttpApi'
import { useOrientation } from '@/composables/useOrientation'
import { useFileTree, type SidebarSettings, FONT_SIZE_MIN, FONT_SIZE_MAX } from '@/composables/useFileTree'
import FileTreeItem from './FileTreeItem.vue'
import FileViewerModal from './FileViewerModal.vue'
import Modal from './Modal.vue'
import Button from './Button.vue'
import { useToast } from '@/composables/useToast'
import { writeClipboardText } from '@/utils/clipboard'

const props = withDefaults(defineProps<{
  sessionId: string
  mode?: 'standalone' | 'emit'
  resizeSide?: 'left' | 'right'
}>(), {
  mode: 'standalone',
  resizeSide: 'left',
})

const emit = defineEmits<{
  'file-select': [name: string, path: string, isDiff: boolean]
}>()

const { t } = useI18n()
const { isLandscape } = useOrientation()
const toast = useToast()
const { tree, loading, error, isDiffMode, expandAll, collapseAll, refresh, toggleDiffMode, settings, updateSettings } = useFileTree(toRef(props, 'sessionId'))

const isRefreshing = ref(false)
const showSettingsPanel = ref(false)
const showFileViewer = ref(false)
const selectedFile = ref('')
const selectedFilePath = ref('')
const fileContent = ref('')
const fileLoading = ref(false)
const fileError = ref<string | null>(null)
const diffLines = ref<FileDiffLine[] | undefined>(undefined)

// ==================== Git Branch Selector ====================
const branches = ref<string[]>([])
const currentBranch = ref<string | null>(null)
const isGitRepo = ref(true)
const branchesLoading = ref(false)
const branchSwitching = ref(false)
const showBranchDropdown = ref(false)

// 分支切换确认弹窗
const showBranchConfirm = ref(false)
const pendingBranch = ref('')
const branchConfirmMsg = ref('')

// 侧边栏宽度（像素），null 表示使用默认百分比
const sidebarWidth = ref<number | null>(null)

// 临时设置状态
const tempDefaultExpanded = ref(false)
const tempFilterText = ref('')
const tempFontSize = ref(FONT_SIZE_MIN)

// 滑块拖动
const sliderTrackRef = ref<HTMLElement | null>(null)
const isDragging = ref(false)

/** 滑块上的刻度点（每 2px 一个点） */
const sliderDots = computed(() => {
  const dots: number[] = []
  for (let i = FONT_SIZE_MIN; i <= FONT_SIZE_MAX; i += 2) {
    dots.push(i)
  }
  return dots
})

/** 字体大小对应的滑块百分比位置 */
const fontSizePercent = computed(() => {
  return ((tempFontSize.value - FONT_SIZE_MIN) / (FONT_SIZE_MAX - FONT_SIZE_MIN)) * 100
})

const sliderFillStyle = computed(() => ({
  width: `${fontSizePercent.value}%`,
}))

const sliderThumbStyle = computed(() => ({
  left: `${fontSizePercent.value}%`,
}))

/** 根据指针在轨道上的位置计算字体大小 */
function fontSizeFromPointer(clientX: number) {
  const track = sliderTrackRef.value
  if (!track) return
  const rect = track.getBoundingClientRect()
  const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width))
  const raw = FONT_SIZE_MIN + ratio * (FONT_SIZE_MAX - FONT_SIZE_MIN)
  // 吸附到整数
  tempFontSize.value = Math.round(Math.max(FONT_SIZE_MIN, Math.min(FONT_SIZE_MAX, raw)))
}

function onSliderPointerDown(e: PointerEvent) {
  isDragging.value = true
  fontSizeFromPointer(e.clientX)
  const track = sliderTrackRef.value
  if (track) track.setPointerCapture(e.pointerId)

  const onMove = (ev: PointerEvent) => {
    if (!isDragging.value) return
    fontSizeFromPointer(ev.clientX)
  }
  const onUp = () => {
    isDragging.value = false
    document.removeEventListener('pointermove', onMove)
    document.removeEventListener('pointerup', onUp)
  }
  document.addEventListener('pointermove', onMove)
  document.addEventListener('pointerup', onUp)
}

const sidebarStyle = computed(() => {
  if (sidebarWidth.value !== null) {
    return { width: `${sidebarWidth.value}px` }
  }
  const widthPercent = isLandscape.value ? '30%' : '40%'
  return { width: widthPercent }
})

async function handleRefresh() {
  isRefreshing.value = true
  await Promise.all([refresh(), loadBranches()])
  if (error.value) {
    toast.error(error.value)
  }
  setTimeout(() => {
    isRefreshing.value = false
  }, 500)
}

async function loadBranches() {
  const { httpGetGitBranches } = useHttpApi()
  branchesLoading.value = true
  try {
    const result = await httpGetGitBranches(props.sessionId)
    if (result.code !== 0 || !result.data) {
      throw new Error(result.message || 'mobile.file.fetchBranchesFailed')
    }
    const data = result.data as GitBranchesData
    isGitRepo.value = data.isGitRepo
    currentBranch.value = data.currentBranch
    branches.value = data.branches
  } catch {
    isGitRepo.value = false
    currentBranch.value = null
    branches.value = []
  } finally {
    branchesLoading.value = false
  }
}

async function switchBranch(branch: string) {
  if (branch === currentBranch.value || branchSwitching.value) return
  showBranchDropdown.value = false

  // 先检查工作区是否有未提交的更改
  try {
    const { httpGetGitStatus } = useHttpApi()
    const result = await httpGetGitStatus(props.sessionId)
    if (result.code === 0 && result.data) {
      const status = result.data as GitStatusData
      if (status.hasChanges) {
        pendingBranch.value = branch
        branchConfirmMsg.value = t('mobile.file.switchConfirmMsg', { count: status.changedCount, branch })
        showBranchConfirm.value = true
        return
      }
    }
  } catch {
    // 状态检查失败时仍允许切换，只是跳过确认
  }

  // 无未提交更改，直接切换
  await doSwitchBranch(branch)
}

/** 确认切换分支 */
async function confirmSwitchBranch() {
  showBranchConfirm.value = false
  if (pendingBranch.value) {
    await doSwitchBranch(pendingBranch.value)
  }
  pendingBranch.value = ''
}

/** 执行分支切换 */
async function doSwitchBranch(branch: string) {
  const { httpGitCheckout } = useHttpApi()
  branchSwitching.value = true
  try {
    const result = await httpGitCheckout(props.sessionId, branch)
    if (result.code !== 0 || !result.data) {
      throw new Error(result.message || 'mobile.file.switchFailed')
    }
    currentBranch.value = result.data.branch
    toast.success(t('mobile.file.switchSuccess', { branch }))
    // 切换分支后自动刷新文件树
    await refresh()
  } catch {
    toast.error(t('mobile.file.switchFailed'))
  } finally {
    branchSwitching.value = false
  }
}

function onBranchClickOutside(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (!target.closest('.branch-selector')) {
    showBranchDropdown.value = false
  }
}

async function handleDiff() {
  toggleDiffMode()
  // 切换模式后统一通过 refresh 获取数据（清除缓存 + fetchTree）
  await refresh()
  if (isDiffMode.value && tree.value.length === 0 && !error.value) {
    toast.info(t('mobile.file.noDiffFilesToast'))
  }
}

function toggleSettings() {
  if (showSettingsPanel.value) {
    cancelSettingsPanel()
  } else {
    // 用当前设置初始化临时状态
    tempDefaultExpanded.value = settings.value.defaultExpanded
    tempFilterText.value = settings.value.filterPatterns.join(', ')
    tempFontSize.value = settings.value.fontSize
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
    fontSize: tempFontSize.value,
  }
  updateSettings(newSettings)
  showSettingsPanel.value = false
}

async function handleFileClick(name: string, path: string) {
  if (props.mode === 'emit') {
    emit('file-select', name, path, isDiffMode.value)
    return
  }

  // standalone 模式
  selectedFile.value = name
  selectedFilePath.value = path
  fileContent.value = ''
  fileError.value = null
  showFileViewer.value = true

  fileLoading.value = true
  try {
    if (isDiffMode.value) {
      const { httpGetFileDiff } = useHttpApi()
      const result = await httpGetFileDiff(props.sessionId, path)
      if (result.code !== 0 || !result.data) {
        throw new Error(result.message || t('mobile.file.fetchDiffFailed'))
      }
      diffLines.value = result.data.lines
    } else {
      const { httpGetFileContent } = useHttpApi()
      const result = await httpGetFileContent(props.sessionId, path)
      if (result.code !== 0 || !result.data) {
        throw new Error(result.message || t('mobile.file.fetchContentFailed'))
      }
      fileContent.value = result.data.content
      diffLines.value = undefined
    }
  } catch (e: any) {
    fileError.value = e?.toString() || t('mobile.file.fetchContentFailed')
  } finally {
    fileLoading.value = false
  }
}

// ==================== Resize Handle ====================

const MIN_SIDEBAR_WIDTH = 150
const MAX_SIDEBAR_RATIO = 0.7

function onResizePointerDown(e: PointerEvent) {
  const startPointerX = e.clientX
  const startWidth = (e.currentTarget as HTMLElement).closest('.file-sidebar')!.getBoundingClientRect().width
  const maxWidth = window.innerWidth * MAX_SIDEBAR_RATIO

  const sidebarEl = (e.currentTarget as HTMLElement).closest('.file-sidebar') as HTMLElement
  sidebarEl.setPointerCapture(e.pointerId)

  const isLeftSide = props.resizeSide === 'left'

  const onMove = (ev: PointerEvent) => {
    let newWidth: number
    if (isLeftSide) {
      // 左侧手柄：左移 = 宽度增大
      newWidth = startWidth - (ev.clientX - startPointerX)
    } else {
      // 右侧手柄：右移 = 宽度增大
      newWidth = startWidth + (ev.clientX - startPointerX)
    }
    sidebarWidth.value = Math.round(Math.max(MIN_SIDEBAR_WIDTH, Math.min(maxWidth, newWidth)))
  }
  const onUp = () => {
    document.removeEventListener('pointermove', onMove)
    document.removeEventListener('pointerup', onUp)
  }
  document.addEventListener('pointermove', onMove)
  document.addEventListener('pointerup', onUp)
}

// ==================== Long Press Copy Path ====================

async function handleLongPress(name: string, path: string) {
  try {
    await writeClipboardText(path)
    toast.success(t('mobile.file.copied', { path }))
  } catch {
    toast.error(t('mobile.file.copyFailed'))
  }
}

// ==================== Branch Lifecycle ====================

watch(toRef(props, 'sessionId'), (newId) => {
  if (newId) {
    loadBranches()
  }
}, { immediate: true })

onMounted(() => {
  document.addEventListener('click', onBranchClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('click', onBranchClickOutside)
})
</script>

<style scoped>
.file-sidebar {
  display: flex;
  flex-direction: column;
  background: var(--mobile-bg-secondary);
  border-left: 1px solid var(--mobile-border);
  flex-shrink: 0;
  overflow: hidden;
  position: relative;
  height: 100%;
}

/* 作为终端页 sidebar overlay 时，覆盖默认 position */
.file-sidebar.sidebar-overlay {
  position: absolute;
}

/* 拖动调整宽度手柄 */
.resize-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 12px;
  cursor: col-resize;
  touch-action: none;
  z-index: 10;
  transition: border-color 0.2s ease;
}

.resize-handle--left {
  left: 0;
  border-left: 2px solid transparent;
}

.resize-handle--right {
  right: 0;
  border-right: 2px solid transparent;
}

.resize-handle:hover,
.resize-handle:active {
  border-color: var(--mobile-accent);
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

/* Branch Selector */
.branch-selector {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  cursor: pointer;
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
  position: relative;
  transition: background-color 0.2s ease;
  max-width: 160px;
  user-select: none;
  -webkit-user-select: none;
}

.branch-selector:hover {
  background: var(--mobile-bg-elevated);
}

.branch-selector:active {
  opacity: 0.8;
}

.branch-icon {
  flex-shrink: 0;
  color: var(--mobile-text-muted);
}

.branch-label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--mobile-text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-transform: none;
  letter-spacing: normal;
}

.branch-label--no-git {
  color: var(--mobile-text-disabled);
  font-weight: 400;
  cursor: default;
}

.branch-label--switching {
  opacity: 0.5;
}

.branch-chevron {
  flex-shrink: 0;
  color: var(--mobile-text-muted);
  transition: transform 0.2s ease;
}

.branch-chevron.open {
  transform: rotate(180deg);
}

.branch-spinner {
  animation: spin 1s linear infinite;
  color: var(--mobile-text-muted);
}

.branch-dropdown {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  min-width: 180px;
  max-width: 280px;
  max-height: 240px;
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.5rem;
  z-index: 40;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.branch-dropdown-title {
  padding: 0.5rem 0.75rem;
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--mobile-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
}

.branch-dropdown-list {
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
}

.branch-dropdown-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 0.5rem 0.75rem;
  background: none;
  border: none;
  color: var(--mobile-text-secondary);
  font-size: 0.8125rem;
  cursor: pointer;
  text-align: left;
  transition: background-color 0.15s ease;
}

.branch-dropdown-item:hover {
  background: var(--mobile-bg-elevated);
}

.branch-dropdown-item:active {
  background: var(--mobile-bg-primary);
}

.branch-dropdown-item.active {
  color: var(--mobile-accent);
  font-weight: 500;
}

.branch-dropdown-item:disabled {
  cursor: default;
  opacity: 0.8;
}

.branch-dropdown-item-name {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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

/* Font Size Slider */
.font-size-value {
  font-size: 0.75rem;
  color: var(--mobile-accent);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.slider-track {
  position: relative;
  height: 28px;
  margin-top: 0.5rem;
  cursor: pointer;
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
}

.slider-fill {
  position: absolute;
  top: 50%;
  left: 0;
  height: 4px;
  transform: translateY(-50%);
  background: var(--mobile-accent);
  border-radius: 2px;
  pointer-events: none;
}

.slider-track::before {
  content: '';
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  height: 4px;
  transform: translateY(-50%);
  background: var(--mobile-bg-elevated);
  border-radius: 2px;
}

.slider-thumb {
  position: absolute;
  top: 50%;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--mobile-accent);
  transform: translate(-50%, -50%);
  box-shadow: 0 0 0 3px rgba(0, 212, 255, 0.2);
  transition: box-shadow 0.15s ease;
  z-index: 2;
}

.slider-thumb:hover {
  box-shadow: 0 0 0 5px rgba(0, 212, 255, 0.3);
}

.slider-dots {
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  transform: translateY(-50%);
  display: flex;
  justify-content: space-between;
  padding: 0 1px;
  pointer-events: none;
  z-index: 1;
}

.slider-dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--mobile-border);
  transition: background 0.15s ease;
}

.slider-dot.active {
  background: var(--mobile-accent);
}

.slider-range-labels {
  display: flex;
  justify-content: space-between;
  margin-top: 0.25rem;
  font-size: 0.6875rem;
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
  color: var(--mobile-text-on-accent);
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
