<template>
  <Teleport to="body">
    <Transition name="bottom-sheet">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-config-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl modal-panel">
        <!-- Header -->
        <div class="flex items-center justify-between p-4 border-b border-[var(--mobile-border)]">
          <span class="font-semibold text-[var(--mobile-text-primary)] text-base">{{ t('mobile.shortcutConfig.title') }}</span>
          <div class="flex items-center gap-2">
            <button
              class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)] transition-colors active:scale-95"
              @click="showHelp = true"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
              </svg>
              {{ t('mobile.shortcutConfig.help') }}
            </button>
            <button
              class="p-1.5 rounded-lg hover:bg-[var(--mobile-accent-muted)] transition-colors"
              @click="emit('close')"
            >
              <svg class="w-5 h-5 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Scrollable Content -->
        <div class="flex-1 overflow-y-auto p-4 space-y-4">
          <!-- 快捷键列表 -->
          <div class="space-y-2">
            <div
              v-for="item in sortedShortcuts"
              :key="item.code"
              class="shortcut-row"
            >
              <span class="shortcut-label" :class="{ 'text-[var(--mobile-text-disabled)]': !item.visible }">
                {{ item.label }}
              </span>
              <div class="flex items-center gap-3">
                <!-- 显示/隐藏开关 -->
                <button
                  class="visibility-toggle"
                  :class="item.visible ? 'active' : 'inactive'"
                  @click="toggleVisibility(item.code)"
                >
                  <span class="toggle-knob" :class="item.visible ? 'on' : 'off'"></span>
                </button>
                <!-- 删除按钮（仅自定义快捷键） -->
                <button
                  v-if="!item.builtin"
                  class="delete-btn"
                  @click="confirmDeleteCode = item.code"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            </div>
          </div>

          <!-- 分隔线 -->
          <div class="border-t border-[var(--mobile-border)]"></div>

          <!-- 添加快捷键 -->
          <div class="space-y-3">
            <span class="text-sm font-medium text-[var(--mobile-text-secondary)]">{{ t('mobile.shortcutConfig.addShortcut') }}</span>

            <!-- 键盘捕获输入框 -->
            <div
              class="capture-input"
              :class="{ 'capture-active': isCapturing }"
              tabindex="0"
              @keydown.capture="handleKeyCapture"
              @focus="isCapturing = true"
              @blur="isCapturing = false"
            >
              <span v-if="previewLabel" class="preview-label">{{ previewLabel }}</span>
              <span v-else class="capture-placeholder">{{ isCapturing ? t('mobile.shortcutConfig.capturing') : t('mobile.shortcutConfig.captureHint') }}</span>
            </div>

            <!-- 修饰键选择 -->
            <div class="space-y-2">
              <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.shortcutConfig.modifierKeys') }}</span>
              <div class="modifier-row">
                <button
                  v-for="mod in modifiers"
                  :key="mod.key"
                  class="modifier-btn"
                  :class="{ selected: activeModifiers[mod.key] }"
                  @click="activeModifiers[mod.key] = !activeModifiers[mod.key]; updatePreview()"
                >
                  {{ mod.label }}
                </button>
              </div>
            </div>

            <!-- 按键选择网格 -->
            <div class="space-y-3">
              <!-- 字母 -->
              <div class="space-y-1">
                <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.shortcutConfig.letters') }}</span>
                <div class="key-grid">
                  <button
                    v-for="letter in letters"
                    :key="letter"
                    class="key-btn"
                    :class="{ selected: selectedKey === letter }"
                    @click="selectKey(letter)"
                  >
                    {{ letter.toUpperCase() }}
                  </button>
                </div>
              </div>

              <!-- 数字 -->
              <div class="space-y-1">
                <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.shortcutConfig.numbers') }}</span>
                <div class="key-grid">
                  <button
                    v-for="num in numbers"
                    :key="num"
                    class="key-btn"
                    :class="{ selected: selectedKey === num }"
                    @click="selectKey(num)"
                  >
                    {{ num }}
                  </button>
                </div>
              </div>

              <!-- 功能键 -->
              <div class="space-y-1">
                <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.shortcutConfig.functionKeys') }}</span>
                <div class="key-grid">
                  <button
                    v-for="fk in functionKeys"
                    :key="fk.code"
                    class="key-btn"
                    :class="{ selected: selectedKey === fk.code }"
                    @click="selectKey(fk.code)"
                  >
                    {{ fk.label }}
                  </button>
                </div>
              </div>

              <!-- 编辑键 -->
              <div class="space-y-1">
                <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.shortcutConfig.editKeys') }}</span>
                <div class="key-grid">
                  <button
                    v-for="ek in editKeys"
                    :key="ek.code"
                    class="key-btn"
                    :class="{ selected: selectedKey === ek.code }"
                    @click="selectKey(ek.code)"
                  >
                    {{ ek.label }}
                  </button>
                </div>
              </div>

              <!-- 方向键 -->
              <div class="space-y-1">
                <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.shortcutConfig.arrowKeys') }}</span>
                <div class="key-grid">
                  <button
                    v-for="ak in arrowKeys"
                    :key="ak.code"
                    class="key-btn"
                    :class="{ selected: selectedKey === ak.code }"
                    @click="selectKey(ak.code)"
                  >
                    {{ ak.label }}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Footer -->
        <div class="p-4 border-t border-[var(--mobile-border)] space-y-2">
          <!-- 重复提示 -->
          <div v-if="isDuplicate" class="duplicate-hint">
            {{ t('mobile.shortcutConfig.alreadyExists') }}
          </div>

          <div class="flex gap-3">
            <button
              class="footer-btn reset-btn"
              @click="handleReset"
            >
              {{ t('mobile.shortcutConfig.resetDefaults') }}
            </button>
            <button
              class="footer-btn add-btn"
              :disabled="!canAdd"
              @click="handleAdd"
            >
              {{ t('mobile.shortcutConfig.confirmAdd') }}
            </button>
          </div>
        </div>
      </div>
    </div>
    </Transition>

      <!-- 删除确认弹窗 -->
      <Transition name="center-modal">
      <div v-if="confirmDeleteCode" class="delete-confirm-overlay" @click.self="confirmDeleteCode = ''">
        <div class="delete-confirm-modal modal-panel">
          <p class="delete-confirm-text">{{ t('mobile.shortcutConfig.deleteConfirm') }}</p>
          <div class="delete-confirm-buttons">
            <button class="delete-confirm-btn cancel" @click="confirmDeleteCode = ''">{{ t('common.button.cancel') }}</button>
            <button class="delete-confirm-btn confirm" @click="handleDelete(confirmDeleteCode)">{{ t('mobile.shortcutConfig.deleteShortcut') }}</button>
          </div>
        </div>
      </div>
      </Transition>

      <!-- 快捷键说明弹窗 -->
      <ShortcutHelpModal :visible="showHelp" @close="showHelp = false" />
    </Teleport>
</template>

<script setup lang="ts">
/**
 * 快捷键配置弹窗
 * 支持键盘捕获和按键网格两种方式添加自定义快捷键
 */
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import type { ShortcutItem } from '@/stores/inputAssistant'
import ShortcutHelpModal from '@/components/ShortcutHelpModal.vue'

const { t } = useI18n()

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const store = useInputAssistantStore()

// ==================== Shortcut List ====================

/** 排序：builtin 在前，自定义在后 */
const sortedShortcuts = computed(() => {
  return [...store.shortcutConfig].sort((a, b) => {
    if (a.builtin && !b.builtin) return -1
    if (!a.builtin && b.builtin) return 1
    return 0
  })
})

function toggleVisibility(code: string) {
  store.toggleShortcutVisibility(code)
}

function handleDelete(code: string) {
  store.removeShortcut(code)
  confirmDeleteCode.value = ''
}

function handleReset() {
  store.resetShortcutConfig()
}

// ==================== Add Shortcut ====================

const isCapturing = ref(false)
const confirmDeleteCode = ref('')
const showHelp = ref(false)
const activeModifiers = ref({
  ctrl: false,
  shift: false,
  alt: false,
})
const selectedKey = ref('')

const modifiers = [
  { key: 'ctrl' as const, label: 'Ctrl' },
  { key: 'shift' as const, label: 'Shift' },
  { key: 'alt' as const, label: 'Alt' },
]

const letters = 'abcdefghijklmnopqrstuvwxyz'.split('')
const numbers = '0123456789'.split('')

const functionKeys = Array.from({ length: 12 }, (_, i) => ({
  code: `f${i + 1}`,
  label: `F${i + 1}`,
}))

const editKeys = [
  { code: 'tab', label: 'Tab' },
  { code: 'enter', label: 'Enter' },
  { code: 'escape', label: 'Esc' },
  { code: 'backspace', label: 'Del' },
  { code: 'delete', label: 'Delete' },
  { code: 'home', label: 'Home' },
  { code: 'end', label: 'End' },
  { code: 'pageup', label: 'PgUp' },
  { code: 'pagedown', label: 'PgDn' },
  { code: 'insert', label: 'Ins' },
  { code: 'space', label: 'Space' },
]

const arrowKeys = [
  { code: 'up', label: '↑' },
  { code: 'down', label: '↓' },
  { code: 'left', label: '←' },
  { code: 'right', label: '→' },
]

// 生成的 code 和 label
const generatedCode = computed(() => {
  if (!selectedKey.value) return ''
  const parts: string[] = []
  if (activeModifiers.value.ctrl) parts.push('ctrl')
  if (activeModifiers.value.shift) parts.push('shift')
  if (activeModifiers.value.alt) parts.push('alt')
  parts.push(selectedKey.value)
  return parts.join('+')
})

const generatedLabel = computed(() => {
  if (!generatedCode.value) return ''
  // 从 code 生成 label：ctrl+a → Ctrl+A, shift+up → Shift+↑
  const parts = generatedCode.value.split('+')
  return parts.map(p => {
    const mod = modifiers.find(m => m.key === p)
    if (mod) return mod.label
    const fk = functionKeys.find(f => f.code === p)
    if (fk) return fk.label
    const ek = editKeys.find(e => e.code === p)
    if (ek) return ek.label
    const ak = arrowKeys.find(a => a.code === p)
    if (ak) return ak.label
    // 单字母大写
    if (p.length === 1) return p.toUpperCase()
    return p
  }).join('+')
})

const previewLabel = computed(() => generatedLabel.value || '')

const isDuplicate = computed(() => {
  if (!generatedCode.value) return false
  return store.shortcutConfig.some(s => s.code === generatedCode.value)
})

const canAdd = computed(() => generatedCode.value && !isDuplicate.value)

function selectKey(code: string) {
  selectedKey.value = selectedKey.value === code ? '' : code
}

function updatePreview() {
  // 触发 computed 重算
}

/** 键盘捕获：keydown 事件映射为 KeyCombo code */
function handleKeyCapture(e: KeyboardEvent) {
  // 忽略纯修饰键按下
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) {
    activeModifiers.value.ctrl = e.ctrlKey
    activeModifiers.value.shift = e.shiftKey
    activeModifiers.value.alt = e.altKey
    e.preventDefault()
    return
  }

  e.preventDefault()
  e.stopPropagation()

  // 映射 KeyboardEvent.key → KeyCombo code
  let key = e.key

  if (key.startsWith('Arrow')) {
    key = key.replace('Arrow', '').toLowerCase() // ArrowUp → up
  } else if (key === ' ') {
    key = 'space'
  } else if (key.startsWith('F') && /^F\d{1,2}$/.test(key)) {
    key = key.toLowerCase() // F1 → f1
  } else if (key === 'Escape') {
    key = 'escape'
  } else if (key === 'Backspace') {
    key = 'backspace'
  } else if (key === 'Delete') {
    key = 'delete'
  } else if (key === 'Tab') {
    key = 'tab'
  } else if (key === 'Enter') {
    key = 'enter'
  } else if (key === 'Home') {
    key = 'home'
  } else if (key === 'End') {
    key = 'end'
  } else if (key === 'PageUp') {
    key = 'pageup'
  } else if (key === 'PageDown') {
    key = 'pagedown'
  } else if (key === 'Insert') {
    key = 'insert'
  } else if (key.length === 1 && /[a-zA-Z0-9]/.test(key)) {
    key = key.toLowerCase()
  } else {
    return
  }

  // 从事件读取修饰键状态
  activeModifiers.value.ctrl = e.ctrlKey
  activeModifiers.value.shift = e.shiftKey
  activeModifiers.value.alt = e.altKey
  selectedKey.value = key
}

function handleAdd() {
  if (!canAdd.value) return
  store.addShortcut(generatedCode.value, generatedLabel.value)
  // 重置输入状态
  selectedKey.value = ''
  activeModifiers.value = { ctrl: false, shift: false, alt: false }
}

// 弹窗打开时重置添加状态
watch(() => props.visible, (show) => {
  if (show) {
    selectedKey.value = ''
    activeModifiers.value = { ctrl: false, shift: false, alt: false }
    isCapturing.value = false
    confirmDeleteCode.value = ''
    showHelp.value = false
  }
})
</script>

<style scoped>
.shortcut-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.5rem;
}

.shortcut-label {
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--mobile-text-primary);
}

.visibility-toggle {
  width: 2.5rem;
  height: 1.375rem;
  border-radius: 9999px;
  position: relative;
  transition: background-color 0.2s ease;
  border: none;
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
}

.visibility-toggle.active {
  background: var(--mobile-accent);
}

.visibility-toggle.inactive {
  background: var(--mobile-bg-secondary);
}

.toggle-knob {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  width: 1rem;
  height: 1rem;
  border-radius: 9999px;
  background: white;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: left 0.2s ease;
}

.toggle-knob.on {
  left: 1.25rem;
}

.toggle-knob.off {
  left: 0.1875rem;
}

.delete-btn {
  width: 1.75rem;
  height: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 0.375rem;
  border: 1px solid var(--mobile-danger-border, var(--mobile-border));
  background: var(--mobile-danger-bg, var(--mobile-bg-elevated));
  color: var(--mobile-danger-color, var(--mobile-text-muted));
  cursor: pointer;
  transition: all 0.15s ease;
}

.delete-btn:active {
  transform: scale(0.9);
}

.capture-input {
  width: 100%;
  min-height: 2.75rem;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 0.75rem;
  cursor: pointer;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  display: flex;
  align-items: center;
}

.capture-input:focus,
.capture-input.capture-active {
  outline: none;
  border-color: var(--mobile-accent);
  box-shadow: 0 0 0 2px var(--mobile-accent-muted);
}

.preview-label {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--mobile-accent);
  font-family: 'Courier New', monospace;
}

.capture-placeholder {
  font-size: 0.8125rem;
  color: var(--mobile-input-placeholder);
}

.modifier-row {
  display: flex;
  gap: 0.5rem;
}

.modifier-btn {
  padding: 0.375rem 0.75rem;
  font-size: 0.75rem;
  font-weight: 500;
  border-radius: 0.5rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
}

.modifier-btn.selected {
  background: var(--mobile-accent-muted);
  border-color: var(--mobile-accent);
  color: var(--mobile-accent);
}

.modifier-btn:active {
  transform: scale(0.95);
}

.key-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(2.25rem, 1fr));
  gap: 0.25rem;
}

.key-btn {
  height: 2rem;
  font-size: 0.6875rem;
  font-weight: 500;
  border-radius: 0.375rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.key-btn.selected {
  background: var(--mobile-accent-muted);
  border-color: var(--mobile-accent);
  color: var(--mobile-accent);
}

.key-btn:active {
  transform: scale(0.93);
}

.duplicate-hint {
  font-size: 0.75rem;
  color: var(--mobile-danger-color, #ff5555);
  text-align: center;
  padding: 0.25rem;
}

.footer-btn {
  flex: 1;
  padding: 0.625rem;
  font-size: 0.8125rem;
  font-weight: 500;
  border-radius: 0.75rem;
  cursor: pointer;
  transition: all 0.15s ease;
}

.reset-btn {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.reset-btn:active {
  transform: scale(0.97);
}

.add-btn {
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
}

.add-btn:active:not(:disabled) {
  transform: scale(0.97);
}

.add-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.delete-confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 110;
  padding: 1rem;
}

.delete-confirm-modal {
  background: var(--mobile-bg-card);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 280px;
  text-align: center;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.delete-confirm-text {
  font-size: 0.9375rem;
  color: var(--mobile-text-primary);
  margin: 0 0 1.25rem;
  line-height: 1.5;
}

.delete-confirm-buttons {
  display: flex;
  gap: 0.75rem;
}

.delete-confirm-btn {
  flex: 1;
  padding: 0.625rem;
  border-radius: 0.625rem;
  font-size: 0.8125rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.delete-confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.delete-confirm-btn.confirm {
  background: #ef4444;
  border: none;
  color: #ffffff;
}

.delete-confirm-btn.confirm:active {
  background: #dc2626;
  transform: scale(0.97);
}
</style>
