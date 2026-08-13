<template>
  <header class="header">
    <button class="back-btn" @click="$emit('back')">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
      </svg>
    </button>
    <div class="header-title-area">
      <h1 class="header-title">{{ sessionName }}</h1>
      <!-- 便捷功能教程入口：跟随标题文字（间距固定），标题截断时保持可见 -->
      <button class="help-btn" @click="$emit('action', 'help')" :title="t('mobile.terminalHelp.title')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="9" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.5 9.5a2.6 2.6 0 115.1 1c-1.1.7-1.6 1.3-1.6 2.6m0 3h.01" />
        </svg>
      </button>
      <transition name="mode-badge">
        <span v-if="isSelectionMode" class="selection-mode-badge">{{ t('mobile.terminal.selectMode') }}</span>
      </transition>
    </div>
    <!-- 常驻工具按钮 -->
    <template v-for="item in visibleItems" :key="item.key">
      <button v-if="item.key === 'task'" class="task-btn" @click="$emit('action', 'task')" :title="t('mobile.terminal.pendingTasks')">
        <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor">
          <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM17.99 9l-1.41-1.42-6.59 6.59-2.58-2.57-1.42 1.41 4 3.99z"/>
        </svg>
      </button>
      <button v-else-if="item.key === 'shortcut'" class="tool-btn" @click="$emit('action', 'shortcut')" :title="t('mobile.shortcutConfig.title')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16M8 6v12M16 6v12" />
        </svg>
      </button>
      <button v-else-if="item.key === 'clear'" class="tool-btn" @click="$emit('action', 'clear')" :title="t('mobile.terminal.clearScreen')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
      <button v-else-if="item.key === 'refresh'" class="tool-btn" @click="$emit('action', 'refresh')" :title="t('mobile.terminal.refreshFormat')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
      <button v-else-if="item.key === 'settings'" class="tool-btn" @click="$emit('action', 'settings')" :title="t('mobile.terminal.settings')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>
      <button v-else-if="item.key === 'folder'" class="folder-btn" :class="{ active: showSidebar }" @click="$emit('action', 'folder')" :title="t('mobile.terminal.files')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
      </button>
    </template>
    <!-- 插件工具栏项 -->
    <PluginTerminalBar />
    <!-- 溢出菜单按钮 -->
    <div v-if="overflowItems.length > 0" class="overflow-menu-wrapper">
      <button class="overflow-btn" :class="{ active: showOverflowMenu }" @click.stop="showOverflowMenu = !showOverflowMenu" :title="t('mobile.terminal.moreTools')">
        <svg class="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/>
        </svg>
      </button>
      <transition name="overflow-menu">
        <div v-if="showOverflowMenu" class="overflow-menu" @click.stop>
          <button v-if="isOverflowItem('task')" class="overflow-menu-item" @click="emitAction('task')">
            <svg viewBox="0 0 24 24" class="w-[18px] h-[18px]" fill="currentColor"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM17.99 9l-1.41-1.42-6.59 6.59-2.58-2.57-1.42 1.41 4 3.99z"/></svg>
            <span>{{ t('mobile.terminal.pendingTasks') }}</span>
          </button>
          <button v-if="isOverflowItem('shortcut')" class="overflow-menu-item" @click="emitAction('shortcut')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16M8 6v12M16 6v12"/></svg>
            <span>{{ t('mobile.shortcutConfig.title') }}</span>
          </button>
          <button v-if="isOverflowItem('clear')" class="overflow-menu-item" @click="emitAction('clear')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
            <span>{{ t('mobile.terminal.clearScreen') }}</span>
          </button>
          <button v-if="isOverflowItem('refresh')" class="overflow-menu-item" @click="emitAction('refresh')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
            <span>{{ t('mobile.terminal.refreshFormat') }}</span>
          </button>
          <button v-if="isOverflowItem('settings')" class="overflow-menu-item" @click="emitAction('settings')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
            <span>{{ t('mobile.terminal.settings') }}</span>
          </button>
          <button v-if="isOverflowItem('folder')" class="overflow-menu-item" :class="{ active: showSidebar }" @click="emitAction('folder')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>
            <span>{{ t('mobile.terminal.files') }}</span>
          </button>
        </div>
      </transition>
    </div>
    <!-- 点击溢出菜单外部关闭 -->
    <div v-if="showOverflowMenu" class="overflow-backdrop" @click="showOverflowMenu = false"></div>
  </header>
</template>

<script setup lang="ts">
/**
 * 终端页头部 - 返回按钮、会话名、工具栏、溢出菜单
 *
 * 工具栏按钮通过 emit('action', key) 通知父组件，不管理弹窗状态
 */
defineOptions({ name: 'TerminalHeader' })

import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ToolbarItemConfig } from '@/components/TerminalSettingsModal.vue'
import PluginTerminalBar from '@/plugin/components/PluginTerminalBar.vue'

const props = defineProps<{
  sessionName: string
  isSelectionMode: boolean
  visibleItems: ToolbarItemConfig[]
  allItems: ToolbarItemConfig[]
  showSidebar: boolean
}>()

const emit = defineEmits<{
  back: []
  action: [key: string]
}>()

const { t } = useI18n()
const showOverflowMenu = ref(false)

const overflowItems = computed(() => {
  const visibleKeys = new Set(props.visibleItems.map(item => item.key))
  return props.allItems.filter(item => !visibleKeys.has(item.key))
})

function isOverflowItem(key: string): boolean {
  return overflowItems.value.some(item => item.key === key)
}

function emitAction(key: string) {
  showOverflowMenu.value = false
  emit('action', key)
}
</script>

<style scoped>
.header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: var(--mobile-terminal-header);
  backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
  /* z-index 高于 movable-area，键盘避让时遮挡上移的终端内容 */
  position: relative;
  z-index: 25;
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
  color: var(--accent, #ece8dc);
}

.help-btn {
  padding: 0.375rem;
  color: var(--mobile-text-muted);
  background: none;
  border: none;
  cursor: pointer;
  transition: color 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.help-btn:hover {
  color: var(--mobile-accent);
}

.header-title-area {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.header-title {
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tool-btn,
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
.task-btn:hover,
.folder-btn:hover,
.overflow-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.task-btn {
  position: relative;
}

.folder-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

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
  /* 自适应宽度：手机窄屏 ~10.5rem，平板放大到 ~13rem，文字永不换行 */
  min-width: clamp(10.5rem, 10.5rem + (100vw - 360px) / 800 * 40, 13rem);
  max-width: min(80vw, 16rem);
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  padding: 0.375rem;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  z-index: 30;
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
  font-size: var(--font-size-base);
  cursor: pointer;
  transition: background 0.15s ease;
  text-align: left;
  white-space: nowrap;
}

.overflow-menu-item:hover {
  background: var(--mobile-bg-hover);
}

.overflow-menu-item.active {
  color: var(--mobile-accent);
}

.overflow-backdrop {
  position: fixed;
  inset: 0;
  z-index: 29;
}

.overflow-menu-enter-active,
.overflow-menu-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.overflow-menu-enter-from,
.overflow-menu-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.95);
}

.selection-mode-badge {
  flex-shrink: 0;
  padding: 0.125rem 0.5rem;
  border-radius: 0.25rem;
  background: color-mix(in srgb, var(--mobile-accent) 20%, transparent);
  color: var(--mobile-accent);
  font-size: var(--font-size-sm);
  font-weight: 500;
  letter-spacing: 0.02em;
  white-space: nowrap;
  border: 1px solid color-mix(in srgb, var(--mobile-accent) 30%, transparent);
}

.mode-badge-enter-active,
.mode-badge-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.mode-badge-enter-from,
.mode-badge-leave-to {
  opacity: 0;
  transform: scale(0.8);
}
</style>
