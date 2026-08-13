<template>
  <Teleport to="body">
    <Transition name="bottom-sheet">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="scm-panel relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg h-[min(85vh,44rem)] flex flex-col shadow-xl modal-panel">
        <!-- 拖拽指示条 -->
        <div class="flex justify-center pt-2">
          <div class="w-10 h-1 rounded-full bg-[var(--mobile-border)]"></div>
        </div>

        <!-- Header -->
        <div class="flex items-center justify-between px-4 pt-2 pb-3">
          <span class="font-semibold text-[var(--mobile-text-primary)] text-base">{{ t('mobile.shortcutConfig.title') }}</span>
          <div class="flex items-center gap-1">
            <button
              class="icon-btn"
              :aria-label="t('mobile.shortcutConfig.help')"
              @click="showHelp = true"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
            </button>
            <button
              class="icon-btn"
              :aria-label="t('common.button.close')"
              @click="emit('close')"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Tab 切换 -->
        <div class="px-4 pb-3">
          <div class="segmented">
            <button
              class="segmented-btn"
              :class="{ active: activeTab === 'list' }"
              @click="activeTab = 'list'"
            >
              {{ t('mobile.shortcutConfig.tabList') }}
              <span class="segmented-badge">{{ store.shortcutConfig.length }}</span>
            </button>
            <button
              class="segmented-btn"
              :class="{ active: activeTab === 'add' }"
              @click="activeTab = 'add'"
            >
              {{ t('mobile.shortcutConfig.tabAdd') }}
            </button>
          </div>
        </div>

        <!-- ==================== Tab: 快捷键列表 ==================== -->
        <template v-if="activeTab === 'list'">
          <div class="flex-1 overflow-y-auto px-4 pb-2" @touchstart="onTouchStart" @touchmove="onTouchMove" @touchend="onTouchEnd">
            <!-- 内置快捷键 -->
            <div class="section-title">{{ t('mobile.shortcutConfig.builtinSection') }}</div>
            <div class="space-y-2">
              <div v-for="item in builtinShortcuts" :key="item.code" class="shortcut-row" :class="{ dimmed: !item.visible }">
                <div class="kbd-group">
                  <template v-for="(part, i) in splitLabel(item.label)" :key="i">
                    <span v-if="i > 0" class="kbd-plus">+</span>
                    <kbd class="kbd-chip">{{ part }}</kbd>
                  </template>
                </div>
                <button
                  class="switch"
                  :class="{ on: item.visible }"
                  :aria-label="item.visible ? t('mobile.shortcutConfig.visible') : t('mobile.shortcutConfig.hidden')"
                  @click="toggleVisibility(item.code)"
                >
                  <span class="switch-knob"></span>
                </button>
              </div>
            </div>

            <!-- 自定义快捷键 -->
            <div class="section-title mt-5">{{ t('mobile.shortcutConfig.customSection') }}</div>
            <div v-if="customShortcuts.length" class="space-y-2">
              <div v-for="item in customShortcuts" :key="item.code" class="shortcut-row" :class="{ dimmed: !item.visible }">
                <div class="kbd-group">
                  <template v-for="(part, i) in splitLabel(item.label)" :key="i">
                    <span v-if="i > 0" class="kbd-plus">+</span>
                    <kbd class="kbd-chip">{{ part }}</kbd>
                  </template>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    class="switch"
                    :class="{ on: item.visible }"
                    :aria-label="item.visible ? t('mobile.shortcutConfig.visible') : t('mobile.shortcutConfig.hidden')"
                    @click="toggleVisibility(item.code)"
                  >
                    <span class="switch-knob"></span>
                  </button>
                  <button
                    class="delete-btn"
                    :aria-label="t('mobile.shortcutConfig.deleteShortcut')"
                    @click="confirmDeleteCode = item.code"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                  </button>
                </div>
              </div>
            </div>
            <div v-else class="empty-hint">{{ t('mobile.shortcutConfig.noCustomHint') }}</div>
          </div>

          <!-- Footer: 恢复默认 -->
          <div class="scm-footer">
            <button class="ghost-btn" @click="handleReset">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              {{ t('mobile.shortcutConfig.resetDefaults') }}
            </button>
          </div>
        </template>

        <!-- ==================== Tab: 添加快捷键 ==================== -->
        <template v-else>
          <div class="flex-1 overflow-y-auto px-4 pb-2 space-y-4" @touchstart="onTouchStart" @touchmove="onTouchMove" @touchend="onTouchEnd">
            <!-- 实时预览 -->
            <div class="preview-box" :class="{ filled: !!previewLabel }">
              <template v-if="previewLabel">
                <div class="kbd-group">
                  <template v-for="(part, i) in splitLabel(previewLabel)" :key="i">
                    <span v-if="i > 0" class="kbd-plus lg">+</span>
                    <kbd class="kbd-chip lg">{{ part }}</kbd>
                  </template>
                </div>
                <button class="clear-btn" :aria-label="t('common.button.clear')" @click="clearSelection">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </template>
              <span v-else class="preview-placeholder">{{ t('mobile.shortcutConfig.previewEmpty') }}</span>
            </div>

            <!-- 键盘捕获 -->
            <div
              class="capture-input"
              :class="{ 'capture-active': isCapturing }"
              tabindex="0"
              @keydown.capture="handleKeyCapture"
              @focus="isCapturing = true"
              @blur="isCapturing = false"
            >
              <svg class="w-4 h-4 shrink-0" :class="isCapturing ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-input-placeholder)]'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v3m0 12v3m9-9h-3M6 12H3m14.5-6.5L15 8m-6 8l-2.5 2.5M17.5 18.5L15 16M9 8L6.5 5.5M21 21H3V3h18v18z" />
              </svg>
              <span :class="isCapturing ? 'capture-active-text' : 'capture-placeholder'">
                {{ isCapturing ? t('mobile.shortcutConfig.capturing') : t('mobile.shortcutConfig.captureHint') }}
              </span>
            </div>

            <!-- 修饰键选择 -->
            <div>
              <div class="section-title">{{ t('mobile.shortcutConfig.modifierKeys') }}</div>
              <div class="modifier-row">
                <button
                  v-for="mod in modifiers"
                  :key="mod.key"
                  class="modifier-btn"
                  :class="{ selected: activeModifiers[mod.key] }"
                  @click="activeModifiers[mod.key] = !activeModifiers[mod.key]"
                >
                  {{ mod.label }}
                </button>
              </div>
            </div>

            <!-- 字母 -->
            <div>
              <div class="section-title">{{ t('mobile.shortcutConfig.letters') }}</div>
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
            <div>
              <div class="section-title">{{ t('mobile.shortcutConfig.numbers') }}</div>
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
            <div>
              <div class="section-title">{{ t('mobile.shortcutConfig.functionKeys') }}</div>
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
            <div>
              <div class="section-title">{{ t('mobile.shortcutConfig.editKeys') }}</div>
              <div class="key-grid wide">
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
            <div>
              <div class="section-title">{{ t('mobile.shortcutConfig.arrowKeys') }}</div>
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

          <!-- Footer: 重复提示 + 添加按钮 -->
          <div class="scm-footer">
            <Transition name="fade">
              <div v-if="isDuplicate" class="duplicate-hint">
                <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
                {{ t('mobile.shortcutConfig.alreadyExists') }}
              </div>
            </Transition>
            <button
              class="add-btn"
              :disabled="!canAdd"
              @click="handleAdd"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
              </svg>
              {{ t('mobile.shortcutConfig.confirmAdd') }}
            </button>
          </div>
        </template>
      </div>
    </div>
    </Transition>

      <!-- 删除确认弹窗 -->
      <Transition name="center-modal">
      <div v-if="deleteTarget" class="delete-confirm-overlay" @click.self="confirmDeleteCode = ''">
        <div class="delete-confirm-modal modal-panel">
          <div class="delete-icon-wrap">
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </div>
          <p class="delete-confirm-text">{{ t('mobile.shortcutConfig.deleteConfirm', { label: deleteTarget.label }) }}</p>
          <div class="delete-confirm-buttons">
            <button class="delete-confirm-btn cancel" @click="confirmDeleteCode = ''">{{ t('common.button.cancel') }}</button>
            <button class="delete-confirm-btn confirm" @click="handleDelete(deleteTarget.code)">{{ t('mobile.shortcutConfig.deleteShortcut') }}</button>
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
 * 双 Tab 结构：列表页管理内置/自定义快捷键，添加页通过键盘捕获或按键网格组合新快捷键
 */
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSwipeTabs } from '@/composables/useSwipeTabs'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import ShortcutHelpModal from '@/components/ShortcutHelpModal.vue'

const { t } = useI18n()

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const store = useInputAssistantStore()

// ==================== Tab 状态 ====================

type Tab = 'list' | 'add'
const activeTab = ref<Tab>('list')

// 内容区左右滑动切换 Tab：左滑 → 添加页，右滑 → 列表页
const { onTouchStart, onTouchMove, onTouchEnd } = useSwipeTabs((dir) => {
  if (dir === 'left' && activeTab.value === 'list') activeTab.value = 'add'
  else if (dir === 'right' && activeTab.value === 'add') activeTab.value = 'list'
})

// ==================== Shortcut List ====================

const builtinShortcuts = computed(() => store.shortcutConfig.filter(s => s.builtin))
const customShortcuts = computed(() => store.shortcutConfig.filter(s => !s.builtin))

/** label 按 '+' 拆分用于渲染键帽 */
function splitLabel(label: string): string[] {
  return label.split('+')
}

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

const confirmDeleteCode = ref('')
const deleteTarget = computed(() =>
  store.shortcutConfig.find(s => s.code === confirmDeleteCode.value)
)
const showHelp = ref(false)

// ==================== Add Shortcut ====================

const isCapturing = ref(false)
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

const canAdd = computed(() => !!generatedCode.value && !isDuplicate.value)

function selectKey(code: string) {
  selectedKey.value = selectedKey.value === code ? '' : code
}

function clearSelection() {
  selectedKey.value = ''
  activeModifiers.value = { ctrl: false, shift: false, alt: false }
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
  clearSelection()
  // 添加成功后回到列表页，给用户即时反馈
  activeTab.value = 'list'
}

// 弹窗打开时重置状态
watch(() => props.visible, (show) => {
  if (show) {
    activeTab.value = 'list'
    clearSelection()
    isCapturing.value = false
    confirmDeleteCode.value = ''
    showHelp.value = false
  }
})
</script>

<style scoped>
/* ==================== 面板与布局 ==================== */

.scm-footer {
  border-top: 1px solid var(--mobile-border);
  padding: 0.75rem 1rem calc(var(--safe-area-bottom, 0px) + 0.75rem);
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.625rem;
  color: var(--mobile-text-muted);
  transition: all 0.15s ease;
}

.icon-btn:hover,
.icon-btn:active {
  background: var(--mobile-accent-muted);
  color: var(--mobile-text-primary);
}

.icon-btn:active {
  transform: scale(0.92);
}

/* ==================== 分段选择器 ==================== */

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
}

.segmented-btn.active {
  background: var(--mobile-bg-card);
  color: var(--mobile-text-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
}

.segmented-badge {
  min-width: 1.25rem;
  height: 1.25rem;
  padding: 0 0.3125rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: var(--font-size-sm);
  font-weight: 600;
  border-radius: 9999px;
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
}

/* ==================== 快捷键列表 ==================== */

.section-title {
  font-size: var(--font-size-sm);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--mobile-text-muted);
  margin-bottom: 0.5rem;
}

.shortcut-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.625rem 0.875rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  transition: opacity 0.2s ease;
}

.shortcut-row.dimmed {
  opacity: 0.5;
}

/* 键帽 */
.kbd-group {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.25rem;
  min-width: 0;
}

.kbd-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1.625rem;
  padding: 0.1875rem 0.4375rem;
  font-family: ui-monospace, 'Cascadia Mono', 'Courier New', monospace;
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--mobile-text-primary);
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-bottom-width: 2px;
  border-radius: 0.375rem;
}

.kbd-chip.lg {
  min-width: 2rem;
  padding: 0.3125rem 0.625rem;
  font-size: var(--font-size-base);
}

.kbd-plus {
  font-size: var(--font-size-sm);
  color: var(--mobile-text-muted);
}

.kbd-plus.lg {
  font-size: var(--font-size-base);
}

/* 显示/隐藏开关 */
.switch {
  position: relative;
  width: 2.5rem;
  height: 1.375rem;
  border-radius: 9999px;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  cursor: pointer;
  flex-shrink: 0;
  transition: background-color 0.2s ease, border-color 0.2s ease;
  padding: 0;
}

.switch.on {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
}

.switch-knob {
  position: absolute;
  top: 50%;
  left: 0.125rem;
  transform: translateY(-50%);
  width: 1rem;
  height: 1rem;
  border-radius: 9999px;
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  transition: left 0.2s ease;
}

.switch.on .switch-knob {
  left: calc(100% - 1.125rem);
}

.delete-btn {
  width: 2rem;
  height: 2rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 0.5rem;
  border: 1px solid var(--mobile-danger-border, var(--mobile-border));
  background: var(--mobile-danger-bg, var(--mobile-bg-elevated));
  color: var(--mobile-danger-color, var(--mobile-text-muted));
  cursor: pointer;
  flex-shrink: 0;
  transition: all 0.15s ease;
}

.delete-btn:active {
  transform: scale(0.9);
}

.empty-hint {
  padding: 1.25rem;
  text-align: center;
  font-size: var(--font-size-sm);
  color: var(--mobile-text-muted);
  background: var(--mobile-bg-elevated);
  border: 1px dashed var(--mobile-border);
  border-radius: 0.75rem;
}

.ghost-btn {
  width: 100%;
  height: 2.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  font-size: var(--font-size-base);
  font-weight: 500;
  color: var(--mobile-text-muted);
  background: transparent;
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  cursor: pointer;
  transition: all 0.15s ease;
}

.ghost-btn:active {
  background: var(--mobile-bg-elevated);
  transform: scale(0.98);
}

/* ==================== 添加页 ==================== */

/* 实时预览 */
.preview-box {
  position: relative;
  min-height: 3.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0.75rem 2.5rem;
  border-radius: 0.75rem;
  border: 1px dashed var(--mobile-border);
  background: var(--mobile-bg-elevated);
  transition: border-color 0.2s ease;
}

.preview-box.filled {
  border-style: solid;
  border-color: var(--mobile-accent);
}

.preview-placeholder {
  font-size: var(--font-size-sm);
  color: var(--mobile-input-placeholder);
  text-align: center;
}

.clear-btn {
  position: absolute;
  top: 50%;
  right: 0.5rem;
  transform: translateY(-50%);
  width: 1.75rem;
  height: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  color: var(--mobile-text-muted);
  transition: all 0.15s ease;
}

.clear-btn:hover,
.clear-btn:active {
  background: var(--mobile-accent-muted);
  color: var(--mobile-text-primary);
}

/* 键盘捕获 */
.capture-input {
  width: 100%;
  min-height: 2.5rem;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.375rem 0.75rem;
  background: var(--mobile-input-bg);
  border: 1px solid var(--mobile-input-border);
  border-radius: 0.75rem;
  cursor: pointer;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.capture-input:focus,
.capture-input.capture-active {
  outline: none;
  border-color: var(--mobile-accent);
  box-shadow: 0 0 0 2px var(--mobile-accent-muted);
}

.capture-placeholder {
  font-size: var(--font-size-sm);
  color: var(--mobile-input-placeholder);
}

.capture-active-text {
  font-size: var(--font-size-sm);
  font-weight: 500;
  color: var(--mobile-accent);
}

/* 修饰键 */
.modifier-row {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.modifier-btn {
  height: 2.5rem;
  font-size: var(--font-size-base);
  font-weight: 500;
  border-radius: 0.625rem;
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

/* 按键网格 - auto-fill 保证各尺寸下按键整齐对齐 */
.key-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(2.25rem, 1fr));
  gap: 0.375rem;
}

.key-grid.wide {
  grid-template-columns: repeat(auto-fill, minmax(3.75rem, 1fr));
}

.key-btn {
  height: 2.375rem;
  font-size: var(--font-size-sm);
  font-weight: 500;
  border-radius: 0.5rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  white-space: nowrap;
}

.key-btn.selected {
  background: var(--mobile-accent-muted);
  border-color: var(--mobile-accent);
  color: var(--mobile-accent);
}

.key-btn:active {
  transform: scale(0.93);
}

/* ==================== 添加页 Footer ==================== */

.duplicate-hint {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  font-size: var(--font-size-sm);
  color: var(--mobile-danger-color, #ff5555);
  margin-bottom: 0.5rem;
}

.add-btn {
  width: 100%;
  height: 2.875rem;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  font-size: var(--font-size-base);
  font-weight: 600;
  border-radius: 0.75rem;
  border: none;
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
  cursor: pointer;
  transition: all 0.15s ease;
}

.add-btn:active:not(:disabled) {
  transform: scale(0.98);
}

.add-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ==================== 删除确认弹窗 ==================== */

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
  border: 1px solid var(--mobile-border);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 20rem;
  text-align: center;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.delete-icon-wrap {
  width: 3rem;
  height: 3rem;
  margin: 0 auto 0.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  background: var(--mobile-danger-bg, rgba(239, 68, 68, 0.1));
  color: var(--mobile-danger-color, #ef4444);
}

.delete-confirm-text {
  font-size: var(--font-size-base);
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
  height: 2.625rem;
  border-radius: 0.625rem;
  font-size: var(--font-size-base);
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
  background: var(--mobile-error);
  border: none;
  color: #ffffff;
}

.delete-confirm-btn.confirm:active {
  transform: scale(0.97);
  filter: brightness(0.9);
}

/* ==================== 过渡动画 ==================== */

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
