<template>
  <Teleport to="body">
    <Transition name="center-modal">
    <div
      v-if="visible"
      class="settings-modal-overlay mobile-ui"
      :style="{ zIndex: zIndex }"
      @click.self="emit('close')"
    >
    <div class="settings-modal modal-panel" :style="modalStyle">
      <div class="settings-header">
        <h2>{{ t('mobile.codeViewer.settingsTitle') }}</h2>
        <button class="close-btn" @click.stop="emit('close')">
          <svg width="24" height="24" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="settings-content">
        <!-- 字体大小 -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.codeViewer.fontSize') }}</label>
          <div class="font-size-control">
            <button class="size-btn" @click.stop="localSettings.fontSize--" :disabled="localSettings.fontSize <= 6">-</button>
            <span class="size-value">{{ localSettings.fontSize }}px</span>
            <button class="size-btn" @click.stop="localSettings.fontSize++" :disabled="localSettings.fontSize >= 24">+</button>
          </div>
        </div>

        <!-- 行间距 -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.codeViewer.lineHeight') }}</label>
          <div class="slider-control">
            <span class="slider-value">{{ localSettings.lineHeight.toFixed(1) }}</span>
            <input
              type="range"
              class="slider-track"
              min="0.5"
              max="2.5"
              step="0.1"
              v-model.number="localSettings.lineHeight"
            />
            <span class="slider-range-label">0.5 – 2.5</span>
          </div>
        </div>

        <!-- 代码主题 -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.codeViewer.codeTheme') }}</label>
          <div class="theme-grid">
            <button
              v-for="(config, id) in CODE_THEMES"
              :key="id"
              class="theme-btn"
              :class="{ active: localSettings.theme === id }"
              @click.stop="localSettings.theme = id"
            >
              <span class="theme-preview" :style="{ background: config.background, color: config.foreground }">Aa</span>
              <span class="theme-name">{{ resolveThemeLabel(config.label, t) }}</span>
            </button>
          </div>
        </div>

        <!-- Tab 缩进 -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.codeViewer.tabIndent') }}</label>
          <div class="tab-size-group">
            <button
              v-for="size in [2, 4, 8]"
              :key="size"
              class="tab-size-btn"
              :class="{ active: localSettings.tabSize === size }"
              @click.stop="localSettings.tabSize = size"
            >{{ size }}</button>
          </div>
        </div>

        <!-- 行号显示 -->
        <div class="settings-section">
          <div class="toggle-row">
            <span class="settings-label" style="margin-bottom:0">{{ t('mobile.codeViewer.lineNumbers') }}</span>
            <button
              class="toggle-btn"
              :class="{ active: localSettings.showLineNumbers }"
              @click.stop="localSettings.showLineNumbers = !localSettings.showLineNumbers"
            >
              <span
                class="toggle-thumb"
                :class="{ on: localSettings.showLineNumbers }"
              ></span>
            </button>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="settings-footer">
        <button class="settings-footer-btn cancel" @click.stop="emit('close')">{{ t('common.button.cancel') }}</button>
        <button class="settings-footer-btn confirm" @click.stop="handleConfirm">{{ t('common.button.confirm') }}</button>
      </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * CodeViewerSettingsModal - 代码查看设置弹窗
 *
 * 支持调整字体大小、代码主题、Tab 缩进和行号显示
 * 编辑中修改临时变量，确认后保存到 store
 */
import { ref, computed, watch, inject, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useCodeViewerStore, CODE_THEMES, type CodeViewerSettings } from '@/stores/codeViewer'
import { resolveThemeLabel } from '@/config/terminalThemes'

const { t } = useI18n()

const props = withDefaults(defineProps<{
  visible: boolean
  /** 覆盖层 z-index，默认 50，嵌套在高层级弹窗中时应传入更高值 */
  zIndex?: number
}>(), {
  zIndex: 50,
})

const emit = defineEmits<{
  close: []
  confirm: [settings: CodeViewerSettings]
}>()

const store = useCodeViewerStore()
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

const localSettings = ref<CodeViewerSettings>({ ...store.settings })

watch(() => props.visible, (show) => {
  if (show) {
    localSettings.value = { ...store.settings }
  }
})

const modalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

function handleConfirm() {
  store.saveSettings(localSettings.value)
  emit('confirm', localSettings.value)
  emit('close')
}
</script>

<style scoped>
/* 与终端设置弹窗保持一致的样式 */
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

/* 字体大小控制 */
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

/* 主题网格 */
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
  box-shadow: 0 0 12px var(--mobile-accent-muted);
}

.theme-preview {
  width: 100%;
  padding: 0.5rem;
  border-radius: 0.375rem;
  text-align: center;
  font-size: 0.875rem;
  font-weight: 600;
  font-family: 'Fira Code', 'JetBrains Mono', monospace;
}

.theme-name {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

.theme-btn.active .theme-name {
  color: var(--mobile-accent);
  font-weight: 600;
}

/* Tab 缩进按钮组 */
.tab-size-group {
  display: flex;
  gap: 0.5rem;
}

.tab-size-btn {
  flex: 1;
  padding: 0.625rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  color: var(--mobile-text-secondary);
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.tab-size-btn:hover {
  background: var(--mobile-bg-hover);
}

.tab-size-btn.active {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  font-weight: 600;
}

/* 行号 toggle */
.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

/* 行间距滑块 */
.slider-control {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.slider-value {
  font-size: 1.125rem;
  font-weight: 500;
  color: var(--mobile-text-primary);
}

.slider-track {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 6px;
  border-radius: 3px;
  background: var(--mobile-bg-elevated);
  outline: none;
  cursor: pointer;
}

.slider-track::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: var(--mobile-accent);
  border: none;
  cursor: pointer;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
  transition: transform 0.15s ease;
}

.slider-track::-webkit-slider-thumb:active {
  transform: scale(1.15);
}

.slider-track::-moz-range-thumb {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: var(--mobile-accent);
  border: none;
  cursor: pointer;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
}

.slider-track::-moz-range-track {
  height: 6px;
  border-radius: 3px;
  background: var(--mobile-bg-elevated);
}

.slider-range-label {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
  text-align: right;
}

.toggle-btn {
  width: 2.75rem;
  height: 1.5rem;
  border-radius: 0.75rem;
  background: var(--mobile-bg-elevated);
  border: none;
  cursor: pointer;
  position: relative;
  transition: background 0.2s ease;
  flex-shrink: 0;
}

.toggle-btn.active {
  background: var(--mobile-accent);
}

.toggle-thumb {
  position: absolute;
  top: 0.125rem;
  left: 0.125rem;
  width: 1.25rem;
  height: 1.25rem;
  border-radius: 50%;
  background: var(--mobile-text-primary);
  box-shadow: 0 1px 3px var(--mobile-overlay-light);
  transition: transform 0.2s ease;
}

.toggle-thumb.on {
  transform: translateX(1.25rem);
}

/* Footer */
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
  opacity: 0.9;
}
</style>
