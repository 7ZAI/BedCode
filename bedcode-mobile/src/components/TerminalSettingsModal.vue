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
              <span class="theme-name">{{ resolveThemeLabel(theme.label, t) }}</span>
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
import { TERMINAL_THEMES, resolveThemeLabel } from '@/config/terminalThemes'

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

// 打开时同步 props 到临时状态
watch(() => props.visible, (visible) => {
  if (visible) {
    tempFontSize.value = props.fontSize
    tempTheme.value = props.isThemeUserSet ? props.theme : 'system'
    tempQuickBarCount.value = props.quickBarCount
    tempToolbarItems.value = [...props.toolbarItems]
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
  padding: 0.5rem 1rem;
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
</style>
