<template>
  <Teleport to="body">
    <Transition name="center-modal">
    <div v-if="visible" class="settings-modal-overlay mobile-ui" @click.self="$emit('cancel')">
      <div class="settings-modal modal-panel" :style="safeAreaStyle">
      <div class="settings-header">
        <h2>{{ t('mobile.terminal.terminalSettings') }}</h2>
        <button class="close-btn" @click.stop="$emit('cancel')">
          <svg width="24" height="24" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Tab 切换：外观 / 杂项配置（减少弹窗高度，同快捷键配置弹窗） -->
      <div class="settings-tabs">
        <div class="segmented">
          <button
            class="segmented-btn"
            :class="{ active: activeTab === 'appearance' }"
            @click="activeTab = 'appearance'"
          >
            {{ t('mobile.terminal.tabAppearance') }}
          </button>
          <button
            class="segmented-btn"
            :class="{ active: activeTab === 'misc' }"
            @click="activeTab = 'misc'"
          >
            {{ t('mobile.terminal.tabMisc') }}
          </button>
        </div>
      </div>

      <!-- ==================== Tab: 外观 ==================== -->
      <template v-if="activeTab === 'appearance'">
      <div class="settings-content" @touchstart="onTouchStart" @touchmove="onTouchMove" @touchend="onTouchEnd">
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
              <span class="theme-name">{{ resolveThemeLabel(theme.label, t) }}</span>
            </button>
          </div>
        </div>
      </div>
      </template>

      <!-- ==================== Tab: 杂项配置 ==================== -->
      <template v-else>
      <div class="settings-content" @touchstart="onTouchStart" @touchmove="onTouchMove" @touchend="onTouchEnd">
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
          <p class="settings-hint">{{ t('mobile.terminal.persistentToolbarHint') }}</p>
          <div class="toolbar-toggle-grid">
            <button
              v-for="item in allToolbarItems"
              :key="item.key"
              class="toolbar-toggle-btn"
              :class="{ active: tempToolbarItems.includes(item.key) }"
              @click.stop="toggleToolbarItem(item.key)"
            >
              <span>{{ t(item.label) }}</span>
            </button>
          </div>
        </div>
      </div>
      </template>

      <!-- Settings Footer -->
      <div class="settings-footer">
        <button class="settings-footer-btn cancel" @click.stop="$emit('cancel')">{{ t('common.button.cancel') }}</button>
        <button class="settings-footer-btn confirm" @click.stop="handleConfirm">{{ t('common.button.confirm') }}</button>
      </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 终端设置弹窗 - 字体大小、主题、快捷栏数量、工具栏配置
 *
 * 所有编辑中的状态 (temp*) 在组件内部管理，确认时通过 emit 传出
 */
defineOptions({ name: 'TerminalSettingsModal' })

import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTheme } from '@/composables/useTheme'
import { useSwipeTabs } from '@/composables/useSwipeTabs'
import { TERMINAL_THEMES, resolveThemeLabel } from '@/config/terminalThemes'

/** 设置分组 Tab：外观（字体/主题）与杂项配置（快捷栏/工具栏） */
type SettingsTab = 'appearance' | 'misc'

export interface TerminalSettings {
  fontSize: number
  theme: string
  isThemeUserSet: boolean
  quickBarCount: number
  toolbarItems: string[]
}

export interface ToolbarItemConfig {
  key: string
  label: string
  icon: string
}

const props = defineProps<{
  visible: boolean
  fontSize: number
  theme: string
  isThemeUserSet: boolean
  quickBarCount: number
  toolbarItems: string[]
  allToolbarItems: ToolbarItemConfig[]
  safeAreaStyle: Record<string, string>
}>()

const emit = defineEmits<{
  confirm: [settings: TerminalSettings]
  cancel: []
}>()

const { t } = useI18n()
const { isSystemDark } = useTheme()

const tempFontSize = ref(props.fontSize)
const tempTheme = ref<string>(props.isThemeUserSet ? props.theme : 'system')
const tempQuickBarCount = ref(props.quickBarCount)
const tempToolbarItems = ref<string[]>([...props.toolbarItems])
const activeTab = ref<SettingsTab>('appearance')

// 内容区左右滑动切换 Tab：左滑 → 杂项配置，右滑 → 外观
const { onTouchStart, onTouchMove, onTouchEnd } = useSwipeTabs((dir) => {
  if (dir === 'left' && activeTab.value === 'appearance') activeTab.value = 'misc'
  else if (dir === 'right' && activeTab.value === 'misc') activeTab.value = 'appearance'
})

// 打开时同步 props 到临时状态
watch(() => props.visible, (visible) => {
  if (visible) {
    tempFontSize.value = props.fontSize
    tempTheme.value = props.isThemeUserSet ? props.theme : 'system'
    tempQuickBarCount.value = props.quickBarCount
    tempToolbarItems.value = [...props.toolbarItems]
    // 每次打开回到「外观」页，避免停留在上一回的分组
    activeTab.value = 'appearance'
  }
})

function getThemePreviewStyle(themeName: string): { background: string; color: string } {
  if (themeName === 'system') {
    const resolved = isSystemDark.value ? 'dark' : 'light'
    const th = TERMINAL_THEMES[resolved]
    return { background: th.background, color: th.foreground }
  }
  const th = TERMINAL_THEMES[themeName]
  return { background: th.background, color: th.foreground }
}

function toggleToolbarItem(key: string) {
  const idx = tempToolbarItems.value.indexOf(key)
  if (idx >= 0) {
    tempToolbarItems.value.splice(idx, 1)
  } else {
    tempToolbarItems.value.push(key)
  }
}

function handleConfirm() {
  let resolvedTheme: string
  let isThemeUserSet: boolean

  if (tempTheme.value === 'system') {
    resolvedTheme = isSystemDark.value ? 'dark' : 'light'
    isThemeUserSet = false
  } else {
    resolvedTheme = tempTheme.value
    isThemeUserSet = true
  }

  emit('confirm', {
    fontSize: tempFontSize.value,
    theme: resolvedTheme,
    isThemeUserSet,
    quickBarCount: tempQuickBarCount.value,
    toolbarItems: tempToolbarItems.value,
  })
}
</script>

<style scoped>
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
  z-index: 50;
  padding: 1rem;
}

.settings-modal {
  --size-btn: clamp(2rem, 2.5rem, 3rem);
  --footer-btn-py: clamp(0.625rem, 0.75rem, 1rem);

  display: flex;
  flex-direction: column;
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 100%;
  max-width: clamp(280px, 360px, 420px);
  /* 固定高度：不随 Tab 切换变化，内容超出由内容区滚动 */
  height: clamp(26rem, 72vh, 34rem);
  overflow: hidden;
}

.settings-tabs {
  padding: 0.75rem 1rem 0;
  flex-shrink: 0;
}

/* 分段控件（同快捷键配置弹窗） */
.segmented {
  display: flex;
  gap: 0.25rem;
  padding: 0.25rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
}

.segmented-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  height: 2.25rem;
  font-size: var(--font-size-sm);
  font-weight: 500;
  border-radius: 0.5rem;
  color: var(--mobile-text-muted);
  transition: all 0.2s ease;
  background: none;
  border: none;
  cursor: pointer;
}

.segmented-btn.active {
  background: var(--mobile-bg-card);
  color: var(--mobile-text-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--mobile-border);
}

.settings-header h2 {
  font-size: var(--font-size-lg);
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
  flex: 1;
  overflow-y: auto;
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
  font-size: var(--font-size-base);
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
  width: var(--size-btn);
  height: var(--size-btn);
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-primary);
  font-size: clamp(1rem, 1.25rem, 1.5rem);
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
  font-size: clamp(1rem, 1.125rem, 1.25rem);
  font-weight: 500;
  color: var(--mobile-text-primary);
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: clamp(0.375rem, 0.5rem, 0.75rem);
}

.theme-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.375rem;
  padding: clamp(0.5rem, 0.75rem, 1rem) clamp(0.375rem, 0.5rem, 0.75rem);
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
  box-shadow: 0 0 12px var(--mobile-accent-muted);
}

.theme-preview {
  width: 100%;
  padding: 0.5rem;
  border-radius: 0.375rem;
  text-align: center;
  font-size: clamp(0.75rem, 0.875rem, 1rem);
  font-weight: 600;
}

.theme-name {
  font-size: clamp(0.625rem, 0.75rem, 0.875rem);
  color: var(--mobile-text-muted);
}

.theme-btn.active .theme-name {
  color: var(--mobile-accent);
  font-weight: 600;
}

.settings-hint {
  font-size: clamp(0.625rem, 0.75rem, 0.875rem);
  color: var(--mobile-text-muted);
  margin: 0 0 0.75rem;
}

.toolbar-toggle-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: clamp(0.375rem, 0.5rem, 0.75rem);
}

.toolbar-toggle-btn {
  padding: clamp(0.375rem, 0.5rem, 0.75rem);
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  color: var(--mobile-text-muted);
  font-size: clamp(0.6875rem, 0.8rem, 0.9375rem);
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
  padding: var(--footer-btn-py);
  border-radius: 0.5rem;
  font-size: clamp(0.8125rem, 0.875rem, 1rem);
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
  opacity: 0.9;
}
</style>
