<template>
  <div
    v-show="isActive"
    class="terminal-view"
    :style="terminalViewStyle"
  >
    <!-- Loading Overlay: 终端初始化期间显示 -->
    <transition name="loading-fade">
      <div v-if="!isTerminalReady" class="loading-overlay">
        <div class="loading-spinner"></div>
        <p class="loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>

    <!-- Header -->
    <header class="header">
      <button class="back-btn" @click="handleBack">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <!-- <div class="status-area">
        <div class="status-dot" :class="statusClass"></div>
        <span class="status-text">{{ statusText }}</span>
      </div> -->
      <div class="header-title-area">
        <h1 class="header-title">{{ sessionName }}</h1>
      </div>
      <!-- 常驻工具按钮：根据配置决定哪些按钮直接显示 -->
      <template v-for="item in visibleToolbarItems" :key="item.key">
        <!-- 临时隐藏：自动/手动模式按钮 -->
        <button v-if="false && item.key === 'mode'" class="mode-btn" :class="{ active: autoMode === 'auto' }" @click="toggleMode" :title="autoMode === 'auto' ? t('mobile.terminal.switchToManual') : t('mobile.terminal.switchToAuto')">
          <svg v-if="autoMode === 'auto'" viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M7 2v11h3v9l7-12h-4l4-8z"/>
          </svg>
          <svg v-else viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm1-14h-2v6l5.25 3.15.75-1.23-4.5-2.67V6z"/>
          </svg>
        </button>
        <!-- 临时隐藏：待办任务按钮 -->
        <button v-else-if="false && item.key === 'task'" class="task-btn" @click="showTaskPicker = true" :title="t('mobile.terminal.pendingTasks')">
          <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM17.99 9l-1.41-1.42-6.59 6.59-2.58-2.57-1.42 1.41 4 3.99z"/>
          </svg>
          <span v-if="hasQueuedTasks" class="task-badge">{{ pendingCount }}</span>
        </button>
        <!-- 临时隐藏：快捷键工具入口按钮 -->
        <button v-else-if="false && item.key === 'shortcut'" class="tool-btn" @click="showShortcutConfig = true" :title="t('mobile.shortcutConfig.title')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
          </svg>
        </button>
        <button v-else-if="item.key === 'clear'" class="tool-btn" @click="confirmClear" :title="t('mobile.terminal.clearScreen')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
        <button v-else-if="item.key === 'refresh'" class="tool-btn" @click="refreshTerminal" :title="t('mobile.terminal.refreshFormat')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
        <button v-else-if="item.key === 'settings'" class="tool-btn" @click="openSettings" :title="t('mobile.terminal.settings')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
        <button v-else-if="item.key === 'folder'" class="folder-btn" :class="{ active: showSidebar }" @click="showSidebar = !showSidebar" :title="t('mobile.terminal.files')">
          <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
        </button>
      </template>
      <!-- 溢出菜单按钮：仅当有非常驻工具时显示 -->
      <div v-if="overflowToolbarItems.length > 0" class="overflow-menu-wrapper">
        <button class="overflow-btn" :class="{ active: showOverflowMenu }" @click.stop="showOverflowMenu = !showOverflowMenu" :title="t('mobile.terminal.moreTools')">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/>
          </svg>
        </button>
        <transition name="overflow-menu">
          <div v-if="showOverflowMenu" class="overflow-menu" @click.stop>
            <!-- 临时隐藏：自动/手动模式 -->
            <button v-if="false && isOverflowItem('mode')" class="overflow-menu-item" :class="{ active: autoMode === 'auto' }" @click="toggleMode()">
              <svg v-if="autoMode === 'auto'" viewBox="0 0 24 24" width="18" height="18" fill="currentColor"><path d="M7 2v11h3v9l7-12h-4l4-8z"/></svg>
              <svg v-else viewBox="0 0 24 24" width="18" height="18" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm1-14h-2v6l5.25 3.15.75-1.23-4.5-2.67V6z"/></svg>
              <span>{{ autoMode === 'auto' ? t('mobile.terminal.autoMode') : t('mobile.terminal.manualMode') }}</span>
              <span class="overflow-item-status">{{ autoMode === 'auto' ? 'ON' : 'OFF' }}</span>
            </button>
            <!-- 临时隐藏：待办任务 -->
            <button v-else-if="false && isOverflowItem('task')" class="overflow-menu-item" @click="showTaskPicker = true">
              <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM17.99 9l-1.41-1.42-6.59 6.59-2.58-2.57-1.42 1.41 4 3.99z"/></svg>
              <span>{{ t('mobile.terminal.pendingTasks') }}</span>
              <span v-if="hasQueuedTasks" class="overflow-item-badge">{{ pendingCount }}</span>
            </button>
            <!-- 临时隐藏：快捷键工具入口 -->
            <button v-else-if="false && isOverflowItem('shortcut')" class="overflow-menu-item" @click="showShortcutConfig = true; closeOverflowMenu()">
              <svg width="18" height="18" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z"/></svg>
              <span>{{ t('mobile.shortcutConfig.title') }}</span>
            </button>
            <button v-if="isOverflowItem('clear')" class="overflow-menu-item" @click="confirmClear()">
              <svg width="18" height="18" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
              <span>{{ t('mobile.terminal.clearScreen') }}</span>
            </button>
            <button v-if="isOverflowItem('refresh')" class="overflow-menu-item" @click="refreshTerminal()">
              <svg width="18" height="18" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
              <span>{{ t('mobile.terminal.refreshFormat') }}</span>
            </button>
            <button v-if="isOverflowItem('settings')" class="overflow-menu-item" @click="openSettings()">
              <svg width="18" height="18" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
              <span>{{ t('mobile.terminal.settings') }}</span>
            </button>
            <button v-if="isOverflowItem('folder')" class="overflow-menu-item" :class="{ active: showSidebar }" @click="showSidebar = !showSidebar">
              <svg width="18" height="18" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>
              <span>{{ t('mobile.terminal.files') }}</span>
            </button>
          </div>
        </transition>
      </div>
      <!-- 点击溢出菜单外部关闭 -->
      <div v-if="showOverflowMenu" class="overflow-backdrop" @click="closeOverflowMenu"></div>
    </header>

    <!-- Main Content: Terminal + Sidebar overlay -->
    <div class="main-content">
      <div class="terminal-output-area">
        <!-- 临时隐藏：自动执行状态条 -->
        <AutoExecuteBar
          v-if="false"
          :current-task="autoCurrentTask"
          :is-paused="autoIsPaused"
          :mode="autoMode"
          @pause="autoPause"
          @resume="autoResume"
        />
        <!-- 触摸滚动容器：接管触摸事件驱动终端滚动 -->
        <div
          ref="scrollContainer"
          class="terminal-scroll-container"
        >
          <div
            ref="xtermContainer"
            class="xterm-container"
            :style="xtermContainerStyle"
          ></div>
          <!-- 自定义滚动条指示器 -->
          <div class="scrollbar-track">
            <div
              class="scrollbar-thumb"
              :class="{ visible: scrollbarVisible }"
              :style="scrollbarThumbStyle"
            ></div>
          </div>
        </div>
      </div>

      <!-- File Sidebar - 覆盖层，不影响终端宽高 -->
      <!-- 始终挂载保持展开状态，通过 CSS 类切换实现滑入/滑出动画 -->
      <FileSidebar
        class="sidebar-overlay"
        :class="{ 'sidebar-hidden': !showSidebar }"
        :session-id="sessionId"
        @long-press="handleLongPress"
      />

      <!-- 点击侧边栏外部关闭 -->
      <div v-if="showSidebar" class="sidebar-backdrop" @click="showSidebar = false"></div>
    </div>

    <!-- Input Bar -->
    <TerminalInputBar
      :disabled="!isSessionActive"
      :is-connected="isConnected"
      :placeholder="inputPlaceholder"
      :is-landscape="isLandscape"
      @submit="handleInputSubmit"
      @execute="handleInputExecute"
      @special-key="handleSpecialKey"
      @shortcuts-panel-toggle="handleShortcutsPanelToggle"
    />

    <!-- Settings Modal -->
    <div v-if="showSettings" class="settings-modal-overlay mobile-ui" @click.self="cancelSettings">
      <div class="settings-modal" :style="settingsModalStyle">
        <div class="settings-header">
          <h2>{{ t('mobile.terminal.terminalSettings') }}</h2>
          <button class="close-btn" @click.stop="cancelSettings">
            <svg width="24" height="24" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="settings-content">
          <!-- Font Size -->
          <div class="settings-section">
            <label class="settings-label">{{ t('mobile.terminal.fontSize') }}</label>
            <div class="font-size-control">
              <button class="size-btn" @click.stop="tempFontSize--" :disabled="tempFontSize <= 10">-</button>
              <span class="size-value">{{ tempFontSize }}px</span>
              <button class="size-btn" @click.stop="tempFontSize++" :disabled="tempFontSize >= 24">+</button>
            </div>
          </div>

          <!-- Theme -->
          <div class="settings-section">
            <label class="settings-label">{{ t('mobile.terminal.theme') }}</label>
            <div class="theme-grid">
              <button
                v-for="(theme, name) in TERMINAL_THEMES"
                :key="name"
                class="theme-btn"
                :class="{ active: tempTheme === name }"
                @click.stop="tempTheme = name"
              >
                <span class="theme-preview" :style="getThemePreviewStyle(name)">Aa</span>
                <span class="theme-name">{{ theme.label }}</span>
              </button>
            </div>
          </div>

          <!-- Quick Bar Count -->
          <div class="settings-section">
            <label class="settings-label">{{ t('mobile.terminal.shortcutCount') }}</label>
            <div class="font-size-control">
              <button class="size-btn" @click.stop="tempQuickBarCount--" :disabled="tempQuickBarCount <= 3">-</button>
              <span class="size-value">{{ tempQuickBarCount }}</span>
              <button class="size-btn" @click.stop="tempQuickBarCount++" :disabled="tempQuickBarCount >= 10">+</button>
            </div>
          </div>

          <!-- Header Toolbar Items -->
          <div class="settings-section">
            <label class="settings-label">{{ t('mobile.terminal.persistentToolbar') }}</label>
            <p class="settings-hint">{{ t('mobile.terminal.persistentToolbar') }}</p>
            <div class="toolbar-toggle-grid">
              <button
                v-for="item in ALL_TOOLBAR_ITEMS"
                :key="item.key"
                class="toolbar-toggle-btn"
                :class="{ active: tempToolbarItems.includes(item.key) }"
                @click.stop="toggleToolbarItem(item.key)"
              >
                <span>{{ item.label }}</span>
              </button>
            </div>
          </div>
        </div>

        <!-- Settings Footer -->
        <div class="settings-footer">
          <button class="settings-footer-btn cancel" @click.stop="cancelSettings">{{ t('common.button.cancel') }}</button>
          <button class="settings-footer-btn confirm" @click.stop="confirmSettings">{{ t('common.button.confirm') }}</button>
        </div>
      </div>
    </div>

    <!-- Clear Confirm Modal -->
    <div v-if="showClearConfirm" class="confirm-modal-overlay mobile-ui" @click.self="showClearConfirm = false">
      <div class="confirm-modal" :style="confirmModalStyle">
        <p class="confirm-text">{{ t('mobile.terminal.clearScreen') }}?</p>
        <div class="confirm-buttons">
          <button class="confirm-btn cancel" @click.stop="showClearConfirm = false">{{ t('common.button.cancel') }}</button>
          <button class="confirm-btn confirm" @click.stop="clearTerminal">{{ t('common.button.confirm') }}</button>
        </div>
      </div>
    </div>
  </div>

  <!-- 任务选择弹窗 -->
  <TaskPickerModal
    v-if="showTaskPicker"
    :tasks="presetTasks"
    @confirm="onTaskConfirm"
    @close="showTaskPicker = false"
  />

  <!-- 快捷键配置弹窗 -->
  <ShortcutConfigModal :visible="showShortcutConfig" @close="showShortcutConfig = false" />
</template>

<script setup lang="ts">
/**
 * 终端视图 - 显示 PTY 输出和输入栏
 * 支持多会话切换和 ANSI 渲染
 */
defineOptions({ name: 'TerminalView' })

import { ref, reactive, computed, inject, type Ref, onMounted, onUnmounted, onActivated, onDeactivated, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import {
  wsJoinSession,
  wsLeaveSession,
  wsSendInput,
  wsResizeTerminal,
} from '@/modules/mobile/composables/useMobileCommands'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import { useTheme } from '@/modules/shared/composables/useTheme'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { useInputAssistantStore } from '@/modules/shared/stores/inputAssistant'
import TerminalInputBar from '@/modules/mobile/components/TerminalInputBar.vue'
import FileSidebar from '@/modules/mobile/components/FileSidebar.vue'
import AutoExecuteBar from '@/modules/mobile/components/AutoExecuteBar.vue'
import TaskPickerModal from '@/modules/mobile/components/TaskPickerModal.vue'
import ShortcutConfigModal from '@/modules/mobile/components/ShortcutConfigModal.vue'
import { useToast } from '@/modules/shared/composables/useToast'
import { writeClipboardText } from '@/modules/shared/utils/clipboard'
import { useAutoExecutor } from '@/modules/mobile/composables/useAutoExecutor'
import { usePresetTasks } from '@/modules/mobile/composables/usePresetTasks'
import type { PresetTask } from '@/modules/mobile/composables/model'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const connection = useMobileConnection()
const toast = useToast()
const { isLandscape } = useOrientation()
const { isSystemDark } = useTheme()
const settingsStore = useSettingsStore()
const assistStore = useInputAssistantStore()
const sessionId = computed(() => route.params.id as string)

// ==================== Auto Executor ====================

const {
  mode: autoMode,
  currentTask: autoCurrentTask,
  isPaused: autoIsPaused,
  hasQueuedTasks,
  pendingTasks: autoPendingTasks,
  setMode: autoSetMode,
  addToQueue,
  pause: autoPause,
  resume: autoResume,
  startNext: autoStartNext,
  handleTaskStatusChanged,
  handleSessionModeChanged,
  cleanup: autoCleanup,
} = useAutoExecutor(sessionId)
const { tasks: presetTasks } = usePresetTasks()

const showTaskPicker = ref(false)
const showOverflowMenu = ref(false)
const pendingCount = computed(() => autoPendingTasks.value.length)

// ==================== Header Toolbar Config ====================

/** 所有可用的 Header 工具项定义 */
const ALL_TOOLBAR_ITEMS = [
  { key: 'mode', label: computed(() => t('mobile.terminal.autoMode')), icon: 'mode' },
  { key: 'task', label: computed(() => t('mobile.terminal.pendingTasks')), icon: 'task' },
  { key: 'shortcut', label: computed(() => t('mobile.shortcutConfig.title')), icon: 'shortcut' },
  { key: 'clear', label: computed(() => t('mobile.terminal.clearScreen')), icon: 'clear' },
  { key: 'refresh', label: computed(() => t('mobile.terminal.refreshFormat')), icon: 'refresh' },
  { key: 'settings', label: computed(() => t('mobile.terminal.settings')), icon: 'settings' },
  { key: 'folder', label: computed(() => t('mobile.terminal.files')), icon: 'folder' },
] as const

/** 常驻显示的工具项（根据配置） */
const visibleToolbarItems = computed(() => {
  const items = assistStore.settings.headerToolbarItems || ['folder']
  return ALL_TOOLBAR_ITEMS.filter(item => items.includes(item.key))
})

/** 收入溢出菜单的工具项 */
const overflowToolbarItems = computed(() => {
  const items = assistStore.settings.headerToolbarItems || ['folder']
  return ALL_TOOLBAR_ITEMS.filter(item => !items.includes(item.key))
})

/** 判断某个工具项是否在溢出菜单中 */
function isOverflowItem(key: string): boolean {
  return overflowToolbarItems.value.some(item => item.key === key)
}

/** 关闭溢出菜单 */
function closeOverflowMenu() {
  showOverflowMenu.value = false
}

/** 切换工具栏常驻项（设置弹窗中使用） */
function toggleToolbarItem(key: string) {
  const idx = tempToolbarItems.value.indexOf(key)
  if (idx >= 0) {
    tempToolbarItems.value.splice(idx, 1)
  } else {
    tempToolbarItems.value.push(key)
  }
}

/** 任务选择确认 */
function onTaskConfirm(tasks: PresetTask[]) {
  addToQueue(tasks)
  showTaskPicker.value = false
  if (autoMode.value === 'auto') {
    autoStartNext()
  }
}

/** 切换自动/手动模式 */
function toggleMode() {
  const newMode = autoMode.value === 'manual' ? 'auto' : 'manual'
  autoSetMode(newMode)
  if (newMode === 'auto' && hasQueuedTasks.value) {
    autoStartNext()
  }
}

// 安全区域从 App.vue inject，不独立初始化 useEdgeToEdge
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!
const keyboardInfo = inject<Ref<{ keyboardHeight: number; isVisible: boolean }>>('keyboardInfo')!


// ==================== State ====================
// 注意：使用 ref 确保每个组件实例有独立的状态
// 在 <script setup> 中，顶层 let 声明的变量是模块级共享的

// keep-alive 可见性：停用时隐藏终端视图，避免 position:fixed 覆盖层拦截触摸事件
const isActive = ref(true)
const xtermContainer = ref<HTMLDivElement | null>(null)
const scrollContainer = ref<HTMLDivElement | null>(null)
// 终端是否准备就绪（初始化 + 订阅完成）
const isTerminalReady = ref(false)
// 终端实例 - 使用 ref 确保组件隔离
const terminalRef = ref<Terminal | null>(null)
const fitAddonRef = ref<FitAddon | null>(null)
const resizeObserverRef = ref<ResizeObserver | null>(null)
// 终端输出事件监听器 - 使用 ref 确保组件隔离
const outputListenerRef = ref<UnlistenFn | null>(null)
// 输出索引去重 - 使用 ref 确保组件隔离
const lastIndexRef = ref(-1)
// 当前订阅的会话 ID - 用于取消订阅时使用（避免路由变化后 sessionId 变成 undefined）
const subscribedSessionIdRef = ref<string | null>(null)
// 订阅进行中标志 - 防止 onActivated 在 subscribeSession 的 await 期间创建重复监听器
const isSubscribing = ref(false)

// 伪滚动容器相关状态
const isUserScrolling = ref(false)
const currentLine = ref(0)
const cellHeight = ref(0)
// 滚动条可见状态：触摸滚动时显示，停止后淡出
const scrollbarVisible = ref(false)
// 触摸滚动状态（reactive 确保每个组件实例独立，避免 script setup 模块级共享）
const touchState = reactive({
  hideTimer: null as ReturnType<typeof setTimeout> | null,
  inertiaRafId: 0,
  startY: 0,
  startLine: 0,
  lastY: 0,
  lastTime: 0,
  velocity: 0,
  // 亚像素累积：保留小数部分，避免 Math.round 丢失微小位移导致滚动不灵敏
  fractionalLine: 0,
})

// 设置相关状态
const showSettings = ref(false)
const showClearConfirm = ref(false)
const showSidebar = ref(false)
const showShortcutConfig = ref(false)
// 自动执行：监听任务状态变更的 Tauri 事件监听器
const taskStatusListenerRef = ref<UnlistenFn | null>(null)
// 监听会话模式变更的 Tauri 事件监听器
const sessionModeListenerRef = ref<UnlistenFn | null>(null)
// 终端主题设置：theme 存储当前生效的主题名，isThemeUserSet 标记是否由用户手动指定
// isThemeUserSet = false 时跟随系统主题变化，true 时保持用户选择
const terminalSettings = ref({
  fontSize: 12,
  theme: (settingsStore.settings.ui.theme === 'system'
    ? (isSystemDark.value ? 'dark' : 'light')
    : settingsStore.settings.ui.theme) as string,
  isThemeUserSet: false,
})

// 临时设置（用于编辑中的状态）
const tempFontSize = ref(12)
const tempTheme = ref<string>(terminalSettings.value.theme)
const tempQuickBarCount = ref(assistStore.settings.quickBarCount)
const tempToolbarItems = ref<string[]>([...(assistStore.settings.headerToolbarItems || ['folder'])])

// 弹窗安全区域样式
const settingsModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

const confirmModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

// ==================== Terminal Themes ====================

const TERMINAL_THEMES: Record<string, any> = {
  system: {
    label: t('settings.appearance.followSystem'),
    // 动态解析，此处仅为占位
    background: 'var(--mobile-terminal-bg)',
    foreground: 'var(--mobile-text-primary)',
  },
  dark: {
    label: t('settings.appearance.darkMode'),
    background: '#0a0a0f',
    foreground: '#e0e0e0',
    cursor: '#00d4ff',
    cursorAccent: '#0a0a0f',
    selectionBackground: '#1a3a4a',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bbbbbb',
    brightBlack: '#555555',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#ffffff',
  },
  light: {
    label: t('settings.appearance.lightMode'),
    background: '#ffffff',
    foreground: '#1a1a1a',
    cursor: '#0066cc',
    cursorAccent: '#ffffff',
    selectionBackground: '#b3d7ff',
    black: '#000000',
    red: '#cc0000',
    green: '#008800',
    yellow: '#996600',
    blue: '#0066cc',
    magenta: '#cc00cc',
    cyan: '#008888',
    white: '#cccccc',
    brightBlack: '#666666',
    brightRed: '#ff0000',
    brightGreen: '#00cc00',
    brightYellow: '#ccaa00',
    brightBlue: '#0088ff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00cccc',
    brightWhite: '#ffffff',
  },
  dracula: {
    label: 'Dracula',
    background: '#282a36',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    cursorAccent: '#282a36',
    selectionBackground: '#44475a',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bfbfbf',
    brightBlack: '#282a36',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#f8f8f2',
  },
  monokai: {
    label: 'Monokai',
    background: '#272822',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    cursorAccent: '#272822',
    selectionBackground: '#49483e',
    black: '#000000',
    red: '#f92672',
    green: '#a6e22e',
    yellow: '#f4bf75',
    blue: '#66d9ef',
    magenta: '#ae81ff',
    cyan: '#a1efe4',
    white: '#f8f8f2',
    brightBlack: '#75715e',
    brightRed: '#f92672',
    brightGreen: '#a6e22e',
    brightYellow: '#f4bf75',
    brightBlue: '#66d9ef',
    brightMagenta: '#ae81ff',
    brightCyan: '#a1efe4',
    brightWhite: '#f9f8f5',
  },
  nord: {
    label: 'Nord',
    background: '#2e3440',
    foreground: '#d8dee9',
    cursor: '#d8dee9',
    cursorAccent: '#2e3440',
    selectionBackground: '#434c5e',
    black: '#3b4252',
    red: '#bf616a',
    green: '#a3be8c',
    yellow: '#ebcb8b',
    blue: '#81a1c1',
    magenta: '#b48ead',
    cyan: '#88c0d0',
    white: '#e5e9f0',
    brightBlack: '#4c566a',
    brightRed: '#bf616a',
    brightGreen: '#a3be8c',
    brightYellow: '#ebcb8b',
    brightBlue: '#81a1c1',
    brightMagenta: '#b48ead',
    brightCyan: '#8fbcbb',
    brightWhite: '#eceff4',
  },
}

// ==================== Settings Functions ====================

/// 获取主题预览样式：'system' 主题根据当前系统状态映射为 dark/light 的颜色
function getThemePreviewStyle(themeName: string): { background: string; color: string } {
  if (themeName === 'system') {
    const resolved = isSystemDark.value ? 'dark' : 'light'
    const t = TERMINAL_THEMES[resolved]
    return { background: t.background, color: t.foreground }
  }
  const t = TERMINAL_THEMES[themeName]
  return { background: t.background, color: t.foreground }
}

function openSettings() {
  // 打开设置时，用当前设置初始化临时状态
  tempFontSize.value = terminalSettings.value.fontSize
  // 如果未手动指定主题，显示 'system'；否则显示实际主题名
  tempTheme.value = terminalSettings.value.isThemeUserSet
    ? terminalSettings.value.theme
    : 'system'
  tempQuickBarCount.value = assistStore.settings.quickBarCount
  tempToolbarItems.value = [...(assistStore.settings.headerToolbarItems || ['folder'])]
  showSettings.value = true
}

function cancelSettings() {
  showSettings.value = false
}

function confirmSettings() {
  // 确认时才应用设置
  terminalSettings.value.fontSize = tempFontSize.value
  // 'system' 解析为当前系统主题，并标记为非用户手动指定
  if (tempTheme.value === 'system') {
    terminalSettings.value.theme = isSystemDark.value ? 'dark' : 'light'
    terminalSettings.value.isThemeUserSet = false
  } else {
    terminalSettings.value.theme = tempTheme.value
    terminalSettings.value.isThemeUserSet = true
  }
  // 保存快捷键条设置
  assistStore.saveSettings({
    quickBarCount: tempQuickBarCount.value,
    headerToolbarItems: tempToolbarItems.value,
  })
  applySettings()
  showSettings.value = false
}

function applySettings() {
  if (!terminalRef.value) return

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]

  // 单独设置每个属性，避免覆盖整个 options 对象
  terminalRef.value.options.theme = theme
  terminalRef.value.options.fontSize = terminalSettings.value.fontSize

  // 重新 fit 终端
  setTimeout(() => fitTerminal(), 50)
}

// ==================== Computed ====================

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)

const session = computed(() =>
  connection.activeSessions.value.find(s => s.id === sessionId.value)
)

const sessionName = computed(() => {
  return session.value?.name || sessionId.value || t('desktop.terminal.title')
})

const sessionStatus = computed(() => {
  return session.value?.status || 'stopped'
})

const isSessionActive = computed(() =>
  sessionStatus.value === 'running'
)

const statusClass = computed(() => {
  if (sessionStatus.value === 'running') return 'status-running'
  if (sessionStatus.value === 'stopped') return 'status-stopped'
  return 'status-unknown'
})

const statusText = computed(() => {
  if (sessionStatus.value === 'running') return t('common.status.running')
  if (sessionStatus.value === 'stopped') return t('common.status.stopped')
  return t('common.status.unknown')
})

const inputPlaceholder = computed(() => {
  if (!isConnected.value) return t('mobile.input.disconnected') + '...'
  if (!isSessionActive.value) return t('mobile.connection.connectFailed')
  return t('mobile.input.commandPlaceholder')
})

// 安全区域
const safeAreaTop = computed(() => safeArea.value.top || 0)
const keyboardHeight = computed(() => keyboardInfo.value.keyboardHeight || 0)

// 快捷键面板偏移：面板展开且终端在底部时，xterm 向上偏移面板高度
const shortcutsPanelHeight = ref(0)

// 终端视图样式：顶部安全区 + 键盘避让
// 底部安全区由 TerminalInputBar 的 paddingBottom 承担，这里只处理键盘避让
// Android WebView 不支持 CSS env(safe-area-inset-*)，完全依赖 JS 值
const terminalViewStyle = computed(() => {
  const bottomOffset = keyboardHeight.value
  return {
    paddingTop: `${safeAreaTop.value}px`,
    paddingBottom: bottomOffset > 0 ? `${bottomOffset}px` : '0px',
  }
})

// xterm 容器偏移样式：快捷键面板展开时向上偏移，避免遮挡底部输出
const xtermContainerStyle = computed(() => {
  if (shortcutsPanelHeight.value <= 0) return {}
  return {
    transform: `translateY(-${shortcutsPanelHeight.value}px)`,
    transition: 'transform 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
  }
})

// 监听键盘变化，重新 fit 终端
// 延迟 300ms 等待系统键盘动画完成后再 resize，避免动画期间重排导致卡顿
watch(() => keyboardInfo.value.keyboardHeight, () => {
  setTimeout(() => fitTerminal(), 300)
})

// 监听外观设置中的主题变化：用户未手动指定终端主题时，跟随外观设置
// 外观设置可选 dark/light/system，终端主题映射为 dark/light
watch(() => settingsStore.settings.ui.theme, (uiTheme) => {
  if (terminalSettings.value.isThemeUserSet) return
  const resolved = uiTheme === 'system'
    ? (isSystemDark.value ? 'dark' : 'light')
    : uiTheme
  if (terminalSettings.value.theme !== resolved) {
    terminalSettings.value.theme = resolved as string
    applySettings()
  }
})

// 监听系统暗色模式变化：仅当外观设置为 system 时才响应
watch(isSystemDark, () => {
  if (terminalSettings.value.isThemeUserSet) return
  if (settingsStore.settings.ui.theme !== 'system') return
  terminalSettings.value.theme = isSystemDark.value ? 'dark' : 'light'
  applySettings()
})

// ==================== Terminal Setup ====================

async function initTerminal() {
  // console.log('[TerminalView] initTerminal called, xtermContainer:', xtermContainer.value)

  if (!xtermContainer.value) {
    // console.error('[TerminalView] xtermContainer is null!')
    return
  }

  // console.log('[TerminalView] Container dimensions:', xtermContainer.value.offsetWidth, 'x', xtermContainer.value.offsetHeight)

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  const term = new Terminal({
    theme: theme,
    fontFamily: '"Courier New", Courier, "Lucida Console", monospace',
    fontSize: terminalSettings.value.fontSize,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    scrollback: 5000,
    convertEol: true,
    // 移动端禁用内置输入，避免弹出输入法
    disableStdin: true,
    // 移动端滚动灵敏度，降低以获得更平滑的触摸滚动
    scrollSensitivity: 0.8,
  })

  terminalRef.value = term
  term.open(xtermContainer.value)

  // Load addons
  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // WebGL renderer - 提升渲染性能
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
    webglAddon.onContextLoss(() => {
      // console.warn('[TerminalView] WebGL context lost')
    })
    // console.log('[TerminalView] WebGL renderer loaded')
  } catch (e) {
    // console.warn('[TerminalView] WebGL not supported, using DOM renderer:', e)
  }

  // Welcome message
  // term.write('\x1b[36m[终端]\x1b[0m ' + sessionName.value + '\r\n')
  // term.write('='.repeat(50) + '\r\n\r\n')

  // Fit terminal - delay to ensure container is rendered
  setTimeout(() => {
    fitTerminal()

    // 配置伪滚动容器
    setupViewportScroll()
  }, 100)

  // Resize observer
  const observer = new ResizeObserver(() => {
    requestAnimationFrame(fitTerminal)
  })
  resizeObserverRef.value = observer
  observer.observe(xtermContainer.value)

  // Window resize
  window.addEventListener('resize', handleWindowResize)

  // Terminal resize 事件：通知桌面端调整 PTY 大小
  // 直接读取 sessionId.value（响应式），不闭包捕获，确保路由切换后使用正确的会话 ID
  term.onResize(({ cols, rows }) => {
    if (isConnected.value && isSessionActive.value && sessionId.value) {
      wsResizeTerminal(sessionId.value, cols, rows).catch((e: Error) => {
        console.warn('[TerminalView] Resize failed:', e)
      })
    }
  })
}

function fitTerminal() {
  if (!fitAddonRef.value || !terminalRef.value) return
  try {
    fitAddonRef.value.fit()
  } catch (e) {
    console.warn('[TerminalView] fit failed:', e)
  }
}

function handleWindowResize() {
  setTimeout(fitTerminal, 100)
}

function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  window.removeEventListener('resize', handleWindowResize)
  // 清理输出监听器
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }
  if (terminalRef.value) {
    terminalRef.value.dispose()
    terminalRef.value = null
    fitAddonRef.value = null
  }
  lastIndexRef.value = -1
  subscribedSessionIdRef.value = null
  isSubscribing.value = false
  isTerminalReady.value = false
  // 清理伪滚动容器状态
  isUserScrolling.value = false
  scrollbarVisible.value = false
  if (touchState.hideTimer) {
    clearTimeout(touchState.hideTimer)
    touchState.hideTimer = null
  }
  if (touchState.inertiaRafId) {
    cancelAnimationFrame(touchState.inertiaRafId)
    touchState.inertiaRafId = 0
  }
  // 清理触摸事件监听器
  if (scrollContainer.value) {
    scrollContainer.value.removeEventListener('touchstart', onTouchStart, { capture: true } as EventListenerOptions)
    scrollContainer.value.removeEventListener('touchmove', onTouchMove, { capture: true } as EventListenerOptions)
    scrollContainer.value.removeEventListener('touchend', onTouchEnd, { capture: true } as EventListenerOptions)
  }
  currentLine.value = 0
  cellHeight.value = 0
}

/// 创建前端事件监听器（不调用后端订阅）
/// 内部方法：统一创建 ws_output 监听器，确保不会重复创建
async function createOutputListener() {
  // 防御性清理：确保不会重复创建
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }

  if (!sessionId.value) {
    return
  }

  try {
    outputListenerRef.value = await listen<{
      session_id: string
      data: string
      index: number
      is_waiting: boolean
    }>('ws_output', (event) => {
      // 直接读取 sessionId.value（响应式），不闭包捕获
      // 避免路由切换后闭包中仍是旧会话 ID，导致新会话输出被过滤
      const currentSessionId = sessionId.value
      if (!currentSessionId) return

      // 只处理当前会话的输出（前端过滤）
      if (event.payload.session_id !== currentSessionId) {
        return
      }

      // 索引去重：避免重复输出（重连时可能发生）
      if (event.payload.index !== undefined && event.payload.index <= lastIndexRef.value) {
        return
      }
      lastIndexRef.value = event.payload.index

      // 写入终端
      if (terminalRef.value) {
        terminalRef.value.write(event.payload.data)
      }
    })
  } catch (e) {
    console.error('[TerminalView] Failed to create output listener:', e)
  }
}

// ==================== Input Handlers ====================

async function subscribeSession() {
  if (!isConnected.value) {
    return
  }

  // 标记订阅进行中，防止 onActivated 创建重复监听器
  isSubscribing.value = true

  try {
    // 先创建前端监听器，再调用后端订阅
    // 后端订阅成功后会立即发送历史输出，如果监听器尚未就绪会丢失
    await createOutputListener()

    // 加入会话，开始接收输出（后端订阅）
    await wsJoinSession(sessionId.value)

    // 保存订阅的会话 ID，用于取消订阅时使用
    subscribedSessionIdRef.value = sessionId.value
  } catch (e) {
    console.error('[TerminalView] Subscribe failed:', e)
    toast.error(t('mobile.connection.connectFailed'))
  } finally {
    isSubscribing.value = false
  }
}

/// 创建前端事件监听器（不调用后端订阅）
/// 用于 onActivated 时恢复前端监听，后端订阅已保持活跃
async function createFrontendListener() {
  await createOutputListener()
}

/// 清理前端事件监听器（不取消后端订阅）
/// 用于组件停用时清理，保持后端订阅以持续接收输出
function clearFrontendListener() {
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }
}

/// 取消订阅会话（包括后端订阅）
/// 用于会话停止、删除或组件销毁时
async function unsubscribeSession() {
  // 使用保存的会话 ID（避免路由变化后 sessionId 变成 undefined）
  const sessionToLeave = subscribedSessionIdRef.value

  // 清理输出监听器
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }

  // 清理保存的会话 ID
  subscribedSessionIdRef.value = null

  // 清理去重索引
  lastIndexRef.value = -1

  if (!isConnected.value || !sessionToLeave) {
    return
  }

  try {
    await wsLeaveSession(sessionToLeave)
  } catch (e) {
    console.error('[TerminalView] Unsubscribe failed:', e)
  }
}

// ==================== Input Handlers ====================

function handleInputSubmit(text: string) {
  if (!terminalRef.value) return

  // 发送输入到桌面端（不带换行，仅输入文本）
  if (isConnected.value && isSessionActive.value) {
    wsSendInput(sessionId.value, text).catch(e => {
      console.error('[TerminalView] Send input failed:', e)
      toast.error(t('mobile.connection.connectFailed'))
    })
  }
}

async function handleInputExecute(text: string) {
  if (!terminalRef.value) return

  // 发送输入到桌面端，然后发送 enter 特殊键执行命令
  if (isConnected.value && isSessionActive.value) {
    try {
      // 先发送文本
      await wsSendInput(sessionId.value, text)
      // 再发送 enter 特殊键
      await wsSendInput(sessionId.value, '', 'enter')
    } catch (e) {
      console.error('[TerminalView] Send input failed:', e)
      toast.error(t('mobile.connection.connectFailed'))
    }
  }
}

function handleSpecialKey(key: string) {
  // 发送特殊键到桌面端
  if (isConnected.value && isSessionActive.value) {
    wsSendInput(sessionId.value, '', key).catch(e => {
      console.error('[TerminalView] Send special key failed:', e)
    })
  }
}

// ==================== Shortcuts Panel ====================

/** 快捷键面板展开/收起时，若终端在底部则向上偏移面板高度，避免遮挡最新输出 */
function handleShortcutsPanelToggle(height: number) {
  if (height > 0) {
    // 仅当终端在底部时才偏移，否则用户已向上滚动，面板不会遮挡关注区域
    if (isScrolledToBottom()) {
      shortcutsPanelHeight.value = height
    }
  } else {
    shortcutsPanelHeight.value = 0
  }
}

// ==================== Clear Terminal ====================

function confirmClear() {
  showClearConfirm.value = true
}

async function clearTerminal() {
  if (!terminalRef.value) return

  terminalRef.value.clear()
  // 清屏后重置滚动状态
  currentLine.value = 0
  isUserScrolling.value = false
  showClearConfirm.value = false
}

// ==================== Refresh Terminal Format ====================

function refreshTerminal() {
  // 刷新格式：重新 fit 终端尺寸并同步到桌面端，不清除内容
  if (!fitAddonRef.value || !terminalRef.value) return

  // 捕获当前 sessionId，避免路由切换后读取错误的会话 ID
  const currentSessionId = sessionId.value

  fitAddonRef.value.fit()
  if (isConnected.value && isSessionActive.value) {
    wsResizeTerminal(currentSessionId, terminalRef.value.cols, terminalRef.value.rows).catch((e: Error) => {
      console.warn('[TerminalView] Refresh resize failed:', e)
    })
  }
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

// ==================== Navigation ====================

function handleBack() {
  router.back()
}

// ==================== Terminal Viewport Scroll Setup ====================

/// 计算单行高度（从 xterm DOM 元素获取）
function computeCellHeight(): number {
  if (!terminalRef.value?.element) return 0
  // 最可靠的方式：viewport 高度 / 可见行数
  const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
  if (viewport && terminalRef.value.rows > 0) {
    return viewport.clientHeight / terminalRef.value.rows
  }
  return 0
}

/// 判断是否滚动到底部
function isScrolledToBottom(): boolean {
  if (!scrollContainer.value || !terminalRef.value) return true
  const maxLine = terminalRef.value.buffer.active.length - terminalRef.value.rows
  // 允许 2 行容差
  return currentLine.value >= maxLine - 2
}

/// 滚动到底部
function scrollToBottom() {
  if (!terminalRef.value) return

  const bufferLength = terminalRef.value.buffer.active.length
  const rows = terminalRef.value.rows
  const targetLine = Math.max(0, bufferLength - rows)

  currentLine.value = targetLine
  terminalRef.value.scrollToLine(targetLine)
  isUserScrolling.value = false
  // 新输出自动滚底时不显示滚动条
}

/// 同步滚动位置到 xterm viewport
function syncViewportToLine(line: number) {
  if (!terminalRef.value) return

  const bufferLength = terminalRef.value.buffer.active.length
  const rows = terminalRef.value.rows
  const maxLine = Math.max(0, bufferLength - rows)

  // 限制范围
  const clampedLine = Math.max(0, Math.min(line, maxLine))
  currentLine.value = clampedLine
  terminalRef.value.scrollToLine(clampedLine)

  // 显示滚动条
  showScrollbar()
}

/// 显示滚动条，并在停止滚动后自动淡出
function showScrollbar() {
  scrollbarVisible.value = true
  if (touchState.hideTimer) {
    clearTimeout(touchState.hideTimer)
  }
  touchState.hideTimer = setTimeout(() => {
    scrollbarVisible.value = false
  }, 1200)
}

/// 滚动条 thumb 样式：根据 currentLine / bufferLength 计算位置和高度
const scrollbarThumbStyle = computed(() => {
  if (!terminalRef.value) return { top: '0%', height: '0%' }

  const bufferLength = terminalRef.value.buffer.active.length
  const rows = terminalRef.value.rows
  if (bufferLength <= 0 || rows <= 0) return { top: '0%', height: '100%' }

  const scrollableLines = bufferLength - rows
  if (scrollableLines <= 0) return { top: '0%', height: '100%' }

  // thumb 高度 = 可见行 / 总行数（最小 20px 对应的百分比，最大 80%）
  const thumbRatio = rows / bufferLength
  const thumbHeight = Math.max(0.08, Math.min(0.8, thumbRatio))

  // thumb 位置 = 当前行 / 可滚动行数 × 可滚动区域
  const scrollRatio = currentLine.value / scrollableLines
  const top = scrollRatio * (1 - thumbHeight)

  return {
    top: `${(top * 100).toFixed(1)}%`,
    height: `${(thumbHeight * 100).toFixed(1)}%`,
  }
})

// ==================== Touch Scroll Handler ====================

function onTouchStart(e: TouchEvent) {
  // 取消惯性滚动
  if (touchState.inertiaRafId) {
    cancelAnimationFrame(touchState.inertiaRafId)
    touchState.inertiaRafId = 0
  }

  const touch = e.touches[0]
  touchState.startY = touch.clientY
  touchState.startLine = currentLine.value
  touchState.lastY = touch.clientY
  touchState.lastTime = Date.now()
  touchState.velocity = 0
  touchState.fractionalLine = 0
}

function onTouchMove(e: TouchEvent) {
  if (!terminalRef.value || cellHeight.value <= 0) return

  const touch = e.touches[0]
  const deltaY = touch.clientY - touchState.lastY
  const deltaTime = Date.now() - touchState.lastTime

  // 计算瞬时速度（像素/毫秒）
  if (deltaTime > 0) {
    touchState.velocity = deltaY / deltaTime
  }

  touchState.lastY = touch.clientY
  touchState.lastTime = Date.now()

  // 将像素距离转换为行数（保留小数，累积小数部分）
  const rawLines = -deltaY / cellHeight.value
  const totalLines = rawLines + touchState.fractionalLine
  const linesDelta = Math.trunc(totalLines)

  if (linesDelta === 0) {
    // 保留累积的小数部分，下次 move 时继续累积
    touchState.fractionalLine = totalLines
    return
  }

  // 消耗整数行后，保留剩余小数
  touchState.fractionalLine = totalLines - linesDelta

  const newLine = currentLine.value + linesDelta

  // 标记用户正在滚动
  isUserScrolling.value = true

  syncViewportToLine(newLine)
}

function onTouchEnd() {
  if (!terminalRef.value || cellHeight.value <= 0) return

  // 启动惯性滚动
  startInertia()
}

/// 惯性滚动：根据松手时的速度逐帧减速
function startInertia() {
  // 速度阈值：太慢则不启动惯性
  if (Math.abs(touchState.velocity) < 0.02) {
    // 惯性结束，检查是否在底部
    if (isScrolledToBottom()) {
      isUserScrolling.value = false
    }
    return
  }

  const friction = 0.97 // 摩擦系数，值越大惯性持续越久

  function step() {
    if (!terminalRef.value || cellHeight.value <= 0) {
      touchState.inertiaRafId = 0
      return
    }

    touchState.velocity *= friction
    // 速度衰减到阈值以下时停止
    if (Math.abs(touchState.velocity) < 0.005) {
      touchState.inertiaRafId = 0
      touchState.fractionalLine = 0
      if (isScrolledToBottom()) {
        isUserScrolling.value = false
      }
      return
    }

    // 速度单位是 像素/毫秒，每帧约 16ms
    const pixelsPerFrame = touchState.velocity * 16
    const rawLines = -pixelsPerFrame / cellHeight.value
    const totalLines = rawLines + touchState.fractionalLine
    const linesPerFrame = Math.trunc(totalLines)

    if (linesPerFrame !== 0) {
      touchState.fractionalLine = totalLines - linesPerFrame
      syncViewportToLine(currentLine.value + linesPerFrame)
    } else {
      touchState.fractionalLine = totalLines
    }

    touchState.inertiaRafId = requestAnimationFrame(step)
  }

  touchState.inertiaRafId = requestAnimationFrame(step)
}

/// 配置触摸滚动：禁用 xterm-viewport 原生滚动，初始化行高
function setupViewportScroll() {
  if (!terminalRef.value?.element) return

  // 禁用 xterm-viewport 的原生滚动
  const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.style.overflowY = 'hidden'
    viewport.style.touchAction = 'none'
    viewport.style.pointerEvents = 'none'
  }

  // 计算行高
  cellHeight.value = computeCellHeight()

  // 在滚动容器上用捕获阶段监听触摸事件，确保即使 xterm 内部 stopPropagation 也能接收到
  if (scrollContainer.value) {
    scrollContainer.value.addEventListener('touchstart', onTouchStart, { passive: true, capture: true })
    scrollContainer.value.addEventListener('touchmove', onTouchMove, { passive: true, capture: true })
    scrollContainer.value.addEventListener('touchend', onTouchEnd, { capture: true })
  }

  // 监听新行输出
  terminalRef.value.onLineFeed(() => {
    // 用户未手动向上滚动时，自动滚到底部
    if (!isUserScrolling.value) {
      nextTick(() => scrollToBottom())
    }
  })

  // 监听 xterm 内部滚动事件，同步 currentLine
  // 当 scrollToLine / scrollToBottom 被调用时，xterm 会触发 onScroll
  terminalRef.value.onScroll((viewportY: number) => {
    currentLine.value = viewportY
  })

  // 监听 resize，更新行高
  terminalRef.value.onResize(() => {
    cellHeight.value = computeCellHeight()
  })

  // 初始滚到底部
  nextTick(() => scrollToBottom())
}

// ==================== Lifecycle ====================

onMounted(async () => {
  await nextTick()
  initTerminal()

  // 首次进入时订阅输出（后端订阅 + 前端监听）
  if (isSessionActive.value && isConnected.value) {
    await subscribeSession()
  }

  isTerminalReady.value = true

  // 监听桌面端推送的任务状态变更事件
  taskStatusListenerRef.value = await listen<{ session_id: string; task_status: string; task_reason?: string; task_questions?: Array<{ header: string; options: Array<{ label: string }> }> }>('ws_sync_task_status_changed', (event) => {
    // 仅处理当前会话的任务状态
    if (event.payload.session_id !== sessionId.value) return
    handleTaskStatusChanged(event.payload.task_status, event.payload.task_questions)
  })

  // 监听桌面端推送的会话模式变更事件（由 /bedcode auto/manual 触发）
  sessionModeListenerRef.value = await listen<{ session_id: string; auto_approve: boolean }>('ws_sync_session_mode_changed', (event) => {
    if (event.payload.session_id !== sessionId.value) return
    handleSessionModeChanged(event.payload.auto_approve)
  })
})

onUnmounted(async () => {
  // 组件销毁时完全取消订阅（包括后端）
  // 注意：keep-alive 缓存的组件不会触发 onUnmounted
  await unsubscribeSession()
  disposeTerminal()
  // 清理任务状态监听器
  if (taskStatusListenerRef.value) {
    taskStatusListenerRef.value()
    taskStatusListenerRef.value = null
  }
  // 清理会话模式监听器
  if (sessionModeListenerRef.value) {
    sessionModeListenerRef.value()
    sessionModeListenerRef.value = null
  }
  autoCleanup()
})

// keep-alive 生命周期：组件被激活时恢复显示
onActivated(async () => {
  // 恢复终端视图可见性（停用时隐藏以避免覆盖层拦截触摸事件）
  isActive.value = true

  // 如果正在订阅中（subscribeSession 的 await 期间），跳过
  if (isSubscribing.value) {
    return
  }
  // 如果没有监听器且会话活跃，重新订阅
  if (isConnected.value && isSessionActive.value && !outputListenerRef.value) {
    await subscribeSession()
  }
  // 延迟执行恢复操作：等 DOM 从 keep-alive 缓存中恢复并完成布局计算
  // 然后自动执行 refreshTerminal 确保终端尺寸和显示正确
  setTimeout(() => {
    // 清除 WebGL 纹理缓存（keep-alive 恢复后纹理可能损坏）
    if (terminalRef.value) {
      terminalRef.value.clearTextureAtlas()
    }
    // 直接调用 refreshTerminal：重新 fit 尺寸并同步到桌面端
    refreshTerminal()
  }, 150)
})

// keep-alive 生命周期：组件被停用时不做任何操作
// 保持前端监听器和后端订阅活跃，让所有终端持续接收输出
onDeactivated(() => {
  // 不取消订阅，不清理监听器
  // 隐藏终端视图，避免 position:fixed 覆盖层拦截触摸事件
  isActive.value = false
})

// Watch session status changes
// 注意：只在组件处于活跃状态且路由正确时才响应状态变化
watch(isSessionActive, async (active, prevActive) => {
  // 如果 sessionId 不存在（路由已离开），忽略状态变化
  if (!sessionId.value) {
    return
  }

  if (active && !prevActive) {
    // Session became active - 需要完整订阅（后端 + 前端）
    await subscribeSession()
  } else if (!active && prevActive) {
    // Session stopped - 完全取消订阅
    await unsubscribeSession()
  }
})

// Watch connection status changes
watch(isConnected, async (connected) => {
  // 如果 sessionId 不存在（路由已离开），忽略连接变化
  if (!sessionId.value) {
    return
  }

  if (!connected) {
    // 连接断开 - 清理前端监听器，后端订阅会自动失效
    clearFrontendListener()
    subscribedSessionIdRef.value = null
  } else if (connected && isSessionActive.value) {
    // 连接恢复 - 需要完整订阅（后端 + 前端）
    await subscribeSession()
  }
})

// Watch sessionId 变化 — keep-alive 可能复用组件实例
// 路由从 /terminal/aaa 切到 /terminal/bbb 时，同一个 TerminalView 实例被复用
// sessionId 变了但 xterm Terminal 和 outputListener 还是旧的
// 需要完整重建：dispose 旧终端、创建新终端、订阅新会话
watch(sessionId, async (newId, oldId) => {
  if (!newId || newId === oldId) return

  isTerminalReady.value = false

  // 取消旧会话的后端订阅
  if (oldId && isConnected.value) {
    try {
      await wsLeaveSession(oldId)
    } catch (e) {
      console.warn('[TerminalView] Leave old session failed:', e)
    }
  }

  // 清理旧终端（dispose xterm + outputListener）
  disposeTerminal()

  // 重建新终端
  await nextTick()
  initTerminal()

  // 订阅新会话
  if (isSessionActive.value && isConnected.value) {
    await subscribeSession()
  }

  isTerminalReady.value = true
})
</script>

<style scoped>
.terminal-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--mobile-terminal-bg);
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
  overflow: hidden;
  /* 不对 padding 做过渡动画：
   * keyboardHeight 是键盘动画结束后的终值（离散跳变），
   * CSS transition 叠加动画会与 Android 系统键盘动画冲突导致卡顿。
   * padding 只做即时响应，由系统键盘动画驱动视觉平滑。 */
}

/* Loading Overlay */
.loading-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
  background: var(--mobile-terminal-bg);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1rem;
}

.loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--mobile-border);
  border-top-color: var(--mobile-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.loading-text {
  font-size: 0.875rem;
  color: var(--mobile-text-muted);
  margin: 0;
}

/* Loading fade transition */
.loading-fade-enter-active,
.loading-fade-leave-active {
  transition: opacity 0.3s ease;
}

.loading-fade-enter-from,
.loading-fade-leave-to {
  opacity: 0;
}

/* Header */
.header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: var(--mobile-terminal-header);
  backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.back-btn {
  padding: 0.5rem;
  margin-left: -0.5rem;
  color: var(--mobile-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  transition: color 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.back-btn:hover {
  color: var(--accent, #00d4ff);
}

.header-title-area {
  flex: 1;
  min-width: 0;
}

.header-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status-area {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 0.75rem;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.status-running {
  background: var(--success, #10b981);
  box-shadow: 0 0 6px rgba(16, 185, 129, 0.5);
}

.status-stopped {
  background: var(--error, #ef4444);
}

.status-unknown {
  background: var(--text-muted, #6b7280);
}

.status-text {
  color: var(--mobile-text-muted);
}

/* 通用工具按钮样式（常驻 + 溢出菜单项） */
.tool-btn,
.mode-btn,
.task-btn,
.folder-btn,
.overflow-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.tool-btn:hover,
.mode-btn:hover,
.task-btn:hover,
.folder-btn:hover,
.overflow-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.mode-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

.task-btn {
  position: relative;
}

.task-badge {
  position: absolute;
  top: -4px;
  right: -4px;
  min-width: 16px;
  height: 16px;
  border-radius: 8px;
  background: var(--mobile-error, #ef4444);
  color: white;
  font-size: 10px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 4px;
}

.folder-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

/* Overflow Menu */
.overflow-menu-wrapper {
  position: relative;
}

.overflow-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

.overflow-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  min-width: 160px;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  padding: 0.375rem;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  z-index: 100;
}

.overflow-menu-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.625rem 0.75rem;
  border-radius: 0.5rem;
  background: none;
  border: none;
  color: var(--mobile-text-primary);
  font-size: 0.875rem;
  cursor: pointer;
  transition: background 0.15s ease;
  text-align: left;
}

.overflow-menu-item:hover {
  background: var(--mobile-bg-hover);
}

.overflow-menu-item.active {
  color: var(--mobile-accent);
}

.overflow-item-status {
  margin-left: auto;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--mobile-text-muted);
}

.overflow-menu-item.active .overflow-item-status {
  color: var(--mobile-accent);
}

.overflow-item-badge {
  margin-left: auto;
  min-width: 18px;
  height: 18px;
  border-radius: 9px;
  background: var(--mobile-error, #ef4444);
  color: white;
  font-size: 10px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 4px;
}

.overflow-backdrop {
  position: fixed;
  inset: 0;
  z-index: 99;
}

/* Overflow menu transition */
.overflow-menu-enter-active,
.overflow-menu-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.overflow-menu-enter-from,
.overflow-menu-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.95);
}

/* Main Content Area */
.main-content {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: hidden;
}

/* Sidebar overlay - 浮动在终端上方，不影响终端宽高 */
.sidebar-overlay {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 20;
  box-shadow: -4px 0 16px rgba(0, 0, 0, 0.3);
}

.sidebar-backdrop {
  position: absolute;
  inset: 0;
  z-index: 15;
}

/* Sidebar Slide - CSS 类驱动动画，始终挂载保持展开状态 */
.sidebar-overlay {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

.sidebar-hidden {
  transform: translateX(100%);
  pointer-events: none;
}

/* Settings Modal */
.settings-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--mobile-overlay-heavy);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 1rem;
}

.settings-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 100%;
  max-width: 360px;
  max-height: 80vh;
  overflow-y: auto;
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--mobile-border);
}

.settings-header h2 {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
}

.close-btn {
  padding: 0.25rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  color: var(--mobile-text-primary);
}

.settings-content {
  padding: 1rem;
}

.settings-section {
  margin-bottom: 1.5rem;
}

.settings-section:last-child {
  margin-bottom: 0;
}

.settings-label {
  display: block;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--mobile-text-muted);
  margin-bottom: 0.75rem;
}

.font-size-control {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.size-btn {
  width: 40px;
  height: 40px;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-primary);
  font-size: 1.25rem;
  cursor: pointer;
  transition: all 0.2s ease;
}

.size-btn:hover:not(:disabled) {
  background: var(--mobile-bg-hover);
}

.size-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.size-value {
  flex: 1;
  text-align: center;
  font-size: 1.125rem;
  font-weight: 500;
  color: var(--mobile-text-primary);
}

.toggle-control {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.toggle-switch {
  width: 2.75rem;
  height: 1.5rem;
  border-radius: 0.75rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  cursor: pointer;
  transition: all 0.2s ease;
  position: relative;
  padding: 0;
}

.toggle-switch.active {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
}

.toggle-knob {
  position: absolute;
  top: 0.125rem;
  left: 0.125rem;
  width: 1.125rem;
  height: 1.125rem;
  border-radius: 50%;
  background: var(--mobile-text-primary);
  transition: transform 0.2s ease;
}

.toggle-switch.active .toggle-knob {
  transform: translateX(1.25rem);
}

.toggle-label {
  font-size: 0.875rem;
  color: var(--mobile-text-muted);
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.theme-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.375rem;
  padding: 0.75rem 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s ease;
}

.theme-btn:hover {
  background: var(--mobile-bg-hover);
}

.theme-btn.active {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.3);
}

.theme-preview {
  width: 100%;
  padding: 0.5rem;
  border-radius: 0.375rem;
  text-align: center;
  font-size: 0.875rem;
  font-weight: 600;
}

.theme-name {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

.theme-btn.active .theme-name {
  color: var(--mobile-accent);
  font-weight: 600;
}

.settings-hint {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
  margin: 0 0 0.75rem;
}

.toolbar-toggle-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.toolbar-toggle-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  color: var(--mobile-text-muted);
  font-size: 0.8rem;
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: center;
}

.toolbar-toggle-btn:hover {
  background: var(--mobile-bg-hover);
}

.toolbar-toggle-btn.active {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  font-weight: 600;
}

.settings-footer {
  display: flex;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--mobile-border);
}

.settings-footer-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-footer-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.settings-footer-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.settings-footer-btn.confirm {
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
}

.settings-footer-btn.confirm:hover {
  background: #00b8e6;
}

/* Confirm Modal */
.confirm-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--mobile-overlay-heavy);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 300px;
  text-align: center;
}

.confirm-text {
  font-size: 1rem;
  color: var(--mobile-text-primary);
  margin: 0 0 1.25rem;
}

.confirm-buttons {
  display: flex;
  gap: 0.75rem;
}

.confirm-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.confirm-btn.confirm {
  background: #ef4444;
  border: none;
  color: #ffffff;
}

.confirm-btn.confirm:hover {
  background: #dc2626;
}

/* Modal Transition */
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-active .settings-modal,
.modal-leave-active .settings-modal,
.modal-enter-active .confirm-modal,
.modal-leave-active .confirm-modal {
  transition: transform 0.2s ease;
}

.modal-enter-from .settings-modal,
.modal-leave-to .settings-modal,
.modal-enter-from .confirm-modal,
.modal-leave-to .confirm-modal {
  transform: scale(0.95);
}

/* Terminal Area - 始终占满 main-content，不被 sidebar 挤压 */
.terminal-output-area {
  position: absolute;
  inset: 0;
  overflow: hidden;
  background: var(--mobile-terminal-bg);
}

/* 伪滚动容器：触摸事件驱动滚动，不需要原生滚动条 */
.terminal-scroll-container {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  touch-action: none;
}

/* 自定义滚动条轨道 */
.scrollbar-track {
  position: absolute;
  top: 4px;
  right: 2px;
  bottom: 4px;
  width: 4px;
  z-index: 5;
  pointer-events: none;
}

/* 自定义滚动条滑块 - 默认隐藏，滚动时淡入 */
.scrollbar-thumb {
  position: absolute;
  left: 0;
  right: 0;
  min-height: 20px;
  border-radius: 2px;
  background: rgba(160, 160, 180, 0.3);
  opacity: 0;
  transition: opacity 0.25s ease, background 0.15s ease;
  pointer-events: none;
}

/* 滚动时显示 */
.scrollbar-thumb.visible {
  opacity: 1;
  background: rgba(0, 212, 255, 0.4);
}

/* xterm 容器：占满滚动容器 */
.xterm-container {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  overflow: hidden;
}

/* xterm 核心样式 - 禁止触摸拖动（由外层滚动容器接管），但允许点击事件（链接等） */
:deep(.xterm) {
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  scrollbar-width: none;
}

:deep(.xterm::-webkit-scrollbar) {
  display: none;
  width: 0;
}

:deep(.xterm-screen) {
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  scrollbar-width: none;
}

:deep(.xterm-screen::-webkit-scrollbar) {
  display: none;
  width: 0;
}

/* 禁用 xterm-viewport 原生滚动，由外层伪滚动容器接管 */
:deep(.xterm-viewport) {
  overflow-y: hidden !important;
  touch-action: none !important;
  pointer-events: none !important;
  scrollbar-width: none !important;
}

:deep(.xterm-viewport::-webkit-scrollbar) {
  display: none !important;
  width: 0 !important;
}

/* 禁用 xterm-scroll-area 滚动条 */
:deep(.xterm-scroll-area) {
  scrollbar-width: none;
}

:deep(.xterm-scroll-area::-webkit-scrollbar) {
  display: none;
  width: 0;
}
</style>