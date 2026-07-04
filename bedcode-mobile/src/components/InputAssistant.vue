<template>
  <div class="fixed z-[999]" :style="floatingStyle">
    <!-- 悬浮球 -->
    <button
      ref="btnRef"
      class="input-assist-btn"
      :class="{ expanded: store.isExpanded }"
      :style="btnStyle"
      @pointerdown.prevent="onPointerDown"
      @pointermove.prevent="onPointerMove"
      @pointerup.prevent="onPointerUp"
      @pointercancel.prevent="onPointerUp"
    >
      <svg class="text-white" :class="iconSize" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
      </svg>
    </button>

    <!-- 径向菜单 -->
    <transition-group
      name="radial"
      tag="div"
      class="radial-container"
      v-show="store.isExpanded"
    >
      <button
        v-for="(item, i) in items"
        :key="item.label"
        class="radial-btn"
        :class="item.cls"
        :style="radialStyle(i)"
        @click.stop="item.action"
      >
        <svg class="w-5 h-5" :class="item.iconCls" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path :d="item.path" />
        </svg>
      </button>
    </transition-group>
  </div>

  <ShortcutPanel :visible="showShortcuts" @close="showShortcuts = false" @select="onShortcut" />
  <SettingsModal :visible="showSettings" @close="showSettings = false" />

  <Teleport to="body">
    <div v-if="showInput" class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui" @click.self="showInput = false">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="showInput = false"></div>
      <div class="relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl w-full max-w-md p-4 shadow-xl">
        <div class="text-sm font-medium mb-3 text-[var(--mobile-text-primary)]">{{ t('mobile.input.commandTitle') }}</div>
        <textarea
          ref="inputRef"
          v-model="inputText"
          class="w-full bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-2 text-sm resize-none focus:outline-none focus:border-[var(--mobile-input-focus)]"
          :placeholder="t('mobile.input.commandPlaceholder')" rows="4"
        ></textarea>
        <div class="flex justify-between gap-2 mt-4">
          <button class="px-4 py-2 text-sm text-[var(--mobile-text-secondary)]" @click="showInput = false">{{ t('common.button.cancel') }}</button>
          <div class="flex gap-2">
            <button class="px-4 py-2 text-sm bg-[var(--mobile-input-bg)] rounded-lg" :disabled="!inputText.trim()" @click="onSubmit">{{ t('common.button.send') }}</button>
            <button class="px-4 py-2 text-sm bg-[var(--mobile-accent)] text-white rounded-lg" :disabled="!inputText.trim()" @click="onExecute">{{ t('common.button.execute') }}</button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import ShortcutPanel from './ShortcutPanel.vue'
import SettingsModal from './SettingsModal.vue'

const { t } = useI18n()

const props = defineProps<{ terminalRef: any; terminalInstance: any; isConnected: boolean }>()
const store = useInputAssistantStore()

// ---------- radial menu items ----------
const items = computed(() => [
  { label: t('mobile.input.clearScreen'), path: 'M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16',                         cls: 'bg-[var(--mobile-bg-card)]', iconCls: 'text-[var(--mobile-text-secondary)]', action: doClear },
  { label: t('mobile.input.input'),       path: 'M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z',           cls: 'bg-[var(--mobile-bg-card)]', iconCls: 'text-[var(--mobile-text-secondary)]', action: doInput },
  { label: 'Ctrl+C',                      path: 'M13 10V3L4 14h7v7l9-11h-7z',                                                                                                      cls: 'bg-[var(--mobile-bg-card)]', iconCls: 'text-amber-500',           action: doCtrlC },
  { label: t('mobile.input.shortcuts'),   path: 'M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z',                     cls: 'bg-[var(--mobile-bg-card)]', iconCls: 'text-[var(--mobile-text-secondary)]', action: doShortcuts },
  { label: t('mobile.terminal.settings'), path: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z', cls: 'bg-[var(--mobile-input-bg)]', iconCls: 'text-[var(--mobile-text-secondary)]', action: doSettings },
])

// ---------- state ----------
const btnRef = ref<HTMLElement>()
const inputRef = ref<HTMLTextAreaElement>()
const showSettings = ref(false)
const showShortcuts = ref(false)
const showInput = ref(false)
const inputText = ref('')

// pointer tracking
const ptr = { downX: 0, downY: 0, downTime: 0, moved: false, dragging: false, movedDist: 0 }

// ---------- computed styles ----------
const S = 48 // button size
const R = 60 // menu radius
const B = 42 // menu button size

const btnStyle = computed(() => ({
  width: `${store.settings.size}px`,
  height: `${store.settings.size}px`,
}))

const iconSize = computed(() => {
  const s = store.settings.size
  return s <= 40 ? 'w-4 h-4' : s <= 48 ? 'w-5 h-5' : 'w-6 h-6'
})

const floatingStyle = computed(() => {
  const p = store.position; const s = store.settings.size; const edge = 16
  return {
    right: `${Math.max(edge, window.innerWidth - Math.min(p.x, window.innerWidth - s - edge) - s)}px`,
    top:  `${Math.max(edge + 60, Math.min(p.y, window.innerHeight - s - edge))}px`,
  }
})

function radialStyle(i: number) {
  // 5 个按钮均匀分布在 120° 上半圆弧 (150°→30°)
  const angle = (150 - 30 * i) * Math.PI / 180
  return {
    left: `${R * Math.cos(angle) + 70 - B / 2}px`,
    top:  `${R * Math.sin(angle) + 70 - B / 2}px`,
    width:  `${B}px`,
    height: `${B}px`,
  }
}

// ---------- pointer handling ----------
function onPointerDown(e: PointerEvent) {
  ptr.downX = e.clientX; ptr.downY = e.clientY
  ptr.downTime = Date.now(); ptr.moved = false; ptr.dragging = false; ptr.movedDist = 0
  if (store.isExpanded) { store.collapse(); return }
  ;(e.target as HTMLElement)?.setPointerCapture?.(e.pointerId)
}

function onPointerMove(e: PointerEvent) {
  const dx = e.clientX - ptr.downX; const dy = e.clientY - ptr.downY
  ptr.movedDist = Math.sqrt(dx * dx + dy * dy)
  if (ptr.movedDist > 8) ptr.moved = true
  if (ptr.movedDist > 12 && Date.now() - ptr.downTime > 200) {
    ptr.dragging = true
    store.savePosition(e.clientX - store.settings.size / 2, e.clientY - store.settings.size / 2)
  }
}

function onPointerUp(e: PointerEvent) {
  try { (e.target as HTMLElement)?.releasePointerCapture?.(e.pointerId) } catch {}
  const dt = Date.now() - ptr.downTime

  if (ptr.dragging) return                                          // drag → just repositioned
  if (ptr.moved && ptr.movedDist > 30 && dt < 500) { doSwipe(e); return }  // swipe gesture
  if (ptr.movedDist < 10 && dt < 300) { toggleMenu(); return }     // tap
  if (ptr.movedDist < 10 && dt >= 300) { showInput.value = true }  // long press → input
}

function toggleMenu() {
  store.isExpanded ? store.collapse() : store.toggleExpanded()
}

function doSwipe(e: PointerEvent) {
  const dx = e.clientX - ptr.downX; const dy = e.clientY - ptr.downY
  const g = store.settings.gestures
  if (Math.abs(dx) > Math.abs(dy)) {
    if (dx > 0 && g.swipeRight) doInput()
    else if (dx < 0 && g.swipeLeft) doShortcuts()
  } else {
    if (dy > 0 && g.swipeDown) doClear()
    else if (dy < 0 && g.swipeUp) doCtrlC()
  }
}

// ---------- actions ----------
function doClear()    { console.log('[InputAssistant] doClear'); store.collapse(); props.terminalRef?.value?.clear() }
function doInput()    { console.log('[InputAssistant] doInput'); store.collapse(); showInput.value = true }
function doCtrlC()    {
  console.log('[InputAssistant] doCtrlC, terminalInstance=', !!props.terminalInstance)
  store.collapse()
  if (!props.isConnected) {
    console.warn('[InputAssistant] Not connected, skip Ctrl+C')
    return
  }
  props.terminalInstance?.sendSpecialKey?.('ctrl_c')
}
function doShortcuts(){ console.log('[InputAssistant] doShortcuts'); store.collapse(); showShortcuts.value = true }
function doSettings() { store.collapse(); showSettings.value = true }

async function onSubmit() {
  const text = inputText.value.trim()
  console.log('[InputAssistant] onSubmit called')
  console.log('[InputAssistant]   text="' + text + '"')
  console.log('[InputAssistant]   terminalInstance=', !!props.terminalInstance)
  console.log('[InputAssistant]   terminalInstance.sessionId=', props.terminalInstance?.sessionId)
  console.log('[InputAssistant]   isConnected=', props.isConnected)

  if (!text) {
    console.warn('[InputAssistant] Empty text, skip submit')
    return
  }
  if (!props.terminalInstance) {
    console.error('[InputAssistant] terminalInstance is undefined!')
    return
  }
  if (!props.isConnected) {
    console.warn('[InputAssistant] Not connected, skip submit')
    return
  }
  try {
    console.log('[InputAssistant] Calling sendInput...')
    await props.terminalInstance.sendInput(text)
    console.log('[InputAssistant] sendInput completed successfully')
  } catch (e) {
    console.error('[InputAssistant] sendInput failed:', e)
    // 不清空输入，让用户可以重试
    return
  }
  inputText.value = ''
  showInput.value = false
}

async function onExecute() {
  const text = inputText.value.trim()
  console.log('[InputAssistant] onExecute called')
  console.log('[InputAssistant]   text="' + text + '"')
  console.log('[InputAssistant]   terminalInstance=', !!props.terminalInstance)
  console.log('[InputAssistant]   terminalInstance.sessionId=', props.terminalInstance?.sessionId)
  console.log('[InputAssistant]   isConnected=', props.isConnected)

  if (!text) {
    console.warn('[InputAssistant] Empty text, skip execute')
    return
  }
  if (!props.terminalInstance) {
    console.error('[InputAssistant] terminalInstance is undefined!')
    return
  }
  if (!props.isConnected) {
    console.warn('[InputAssistant] Not connected, skip execute')
    return
  }
  if (!props.terminalInstance.sendInputWithEnter) {
    console.error('[InputAssistant] sendInputWithEnter method not found!')
    return
  }
  try {
    console.log('[InputAssistant] Calling sendInputWithEnter...')
    // 一次发送同时携带文本和 Enter，避免两次 invoke 的竞态条件
    // 桌面端 Input handler 按 data → special_key 顺序写入 PTY
    await props.terminalInstance.sendInputWithEnter(text)
    console.log('[InputAssistant] sendInputWithEnter completed successfully')
  } catch (e) {
    console.error('[InputAssistant] sendInputWithEnter failed:', e)
    // 不清空输入，让用户可以重试
    return
  }
  inputText.value = ''
  showInput.value = false
}

async function onShortcut(key: string) {
  console.log('[InputAssistant] onShortcut called, key="' + key + '", terminalInstance=', !!props.terminalInstance)
  if (!props.isConnected) {
    console.warn('[InputAssistant] Not connected, skip shortcut')
    return
  }
  try {
    await props.terminalInstance?.sendSpecialKey?.(key)
  } catch (e) {
    console.error('[InputAssistant] sendSpecialKey failed:', e)
  }
}

// auto-focus textarea
watch(showInput, async v => { if (v) { inputText.value = ''; await nextTick(); inputRef.value?.focus() } })

// resize tracker
function onResize() { /* triggers floatingStyle recompute */ }
onMounted(() => window.addEventListener('resize', onResize))
onUnmounted(() => window.removeEventListener('resize', onResize))
</script>

<style scoped>
.input-assist-btn {
  position: relative;
  z-index: 10;
  border-radius: 9999px;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 4px 12px rgba(0,0,0,0.25);
  touch-action: none;
  cursor: pointer;
  background: var(--mobile-input-assist-bg);
  transition: transform 0.15s ease, box-shadow 0.15s ease;
}
.input-assist-btn:active {
  transform: scale(0.92);
}

/* radial menu container */
.radial-container {
  position: absolute;
  width: 140px; height: 140px;
  left: 50%; top: 50%;
  transform: translate(-50%, -50%);
  pointer-events: none;
}

/* individual radial button */
.radial-btn {
  position: absolute;
  border-radius: 9999px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.15);
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: auto;
  cursor: pointer;
  transition: transform 0.15s ease;
}
.radial-btn:active { transform: scale(0.85) !important; }

/* radial enter/leave animation */
.radial-enter-active {
  transition: all 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.radial-leave-active {
  transition: all 0.12s ease-in;
}
.radial-enter-from,
.radial-leave-to {
  opacity: 0;
  transform: scale(0.3) !important;
}
.radial-enter-active:nth-child(1) { transition-delay: 0ms; }
.radial-enter-active:nth-child(2) { transition-delay: 30ms; }
.radial-enter-active:nth-child(3) { transition-delay: 60ms; }
.radial-enter-active:nth-child(4) { transition-delay: 90ms; }
.radial-enter-active:nth-child(5) { transition-delay: 120ms; }
.radial-leave-active:nth-child(1) { transition-delay: 120ms; }
.radial-leave-active:nth-child(2) { transition-delay: 90ms; }
.radial-leave-active:nth-child(3) { transition-delay: 60ms; }
.radial-leave-active:nth-child(4) { transition-delay: 30ms; }
.radial-leave-active:nth-child(5) { transition-delay: 0ms; }
</style>
