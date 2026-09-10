<template>
  <Teleport to="body">
    <Transition name="pgd">
      <div
        v-if="entry"
        class="pgd-overlay mobile-ui"
        role="dialog"
        aria-modal="true"
        @click.self="onBackdrop"
      >
        <div class="pgd-card" :class="entry.widthClass ?? 'max-w-md'">
          <!-- 预设模式：标题 + 倒计时 + 正文 + 动作按钮 -->
          <template v-if="!entry.content">
            <div class="pgd-head px-5 pt-5" :class="{ 'pgd-head--closable': entry.closable !== false }">
              <span class="pgd-title">{{ entry.title }}</span>
              <span
                v-if="countdownText"
                class="pgd-countdown"
                :class="{ 'pgd-countdown--urgent': countdownSeconds !== null && countdownSeconds <= 10 }"
              >
                {{ countdownText }}
              </span>
            </div>
            <div class="px-5 py-3">
              <div v-if="entry.message" class="pgd-message whitespace-pre-line">{{ entry.message }}</div>
              <div
                v-else-if="!entry.actions?.length && !countdownText"
                class="pgd-message pgd-message--empty"
              >
                {{ '' }}
              </div>
            </div>
            <div v-if="entry.actions?.length" class="pgd-actions px-5 pb-5">
              <button
                v-for="(action, i) in entry.actions"
                :key="i"
                class="pgd-btn"
                :class="`pgd-btn--${action.kind ?? 'default'}`"
                :disabled="action.disabled"
                @click="onAction(action)"
              >
                {{ action.label }}
              </button>
            </div>
          </template>

          <!-- 组件模式：任意插件组件 + pluginContext -->
          <content-shell v-else :entry="entry" :class="entry.bodyClass ?? 'p-0'" />

          <!-- 关闭按钮（closable 时） -->
          <button
            v-if="entry.closable !== false"
            class="pgd-close"
            :aria-label="''"
            @click="closeCurrent"
          >
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" class="pgd-close-ico">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * PluginGlobalDialog — 移动端宿主全局弹窗（通用能力，宿主 + dev-shell 共用）
 *
 * 与桌面端同构：
 * - 预设模式：title / message / icon / actions
 * - 组件模式：content 任意 Vue 组件（provide('pluginContext')）
 * - FIFO 排队（global-dialog 控制器）、可选定时自动关闭 + 倒计时
 *
 * 移动端样式适配：使用 --mobile-*  design token，卡片圆角 16px，适配安全区，
 * 按钮触摸目标 44px，文案换行 whitespace-pre-line 支持指纹等多行提示。
 *
 * 主题作用域：浅色 token 定义在 `html:not(.dark) .mobile-ui`（mobile.css），
 * 而本组件 Teleport 到 body（逃逸 .mobile-ui 布局根），故 overlay 根元素必须
 * 挂 `mobile-ui` 类才能吃到浅色/色板 token——与 FsAuthDialog 同模式；
 * 深色为 :root 默认值不受影响。按钮随 --mobile-accent 深浅翻转（跟随主题色）。
 *
 * 渲染完全由 getGlobalDialog / subscribeGlobalDialog 驱动。
 */
import { computed, defineComponent, h, onBeforeUnmount, onMounted, provide, ref, watch } from 'vue'
import {
  closeGlobalDialog,
  getGlobalDialog,
  resolveDialogDeadline,
  subscribeGlobalDialog,
  type GlobalDialogEntry,
} from '../global-dialog'
import { getRouter } from '../runtime'
import type { PluginDialogAction } from '../types'

const entry = ref<GlobalDialogEntry | null>(getGlobalDialog())
let unsubscribe: (() => void) | null = null

const now = ref(Date.now())
let tickTimer: ReturnType<typeof setInterval> | null = null

const countdownSeconds = computed<number | null>(() => {
  const e = entry.value
  if (!e) return null
  const deadline = resolveDialogDeadline(e)
  if (deadline == null) return null
  return Math.max(0, Math.ceil((deadline - now.value) / 1000))
})

const countdownText = computed<string>(() => {
  const e = entry.value
  const seconds = countdownSeconds.value
  // 倒计时可用判定：标签须含 {seconds} 占位符且秒数可计算，否则整行提示不渲染
  // （无占位符的静态文案会与正文重复；秒数不可用说明无 deadline，无倒计时可言）
  if (!e?.countdownLabel || !e.countdownLabel.includes('{seconds}') || seconds == null) return ''
  return e.countdownLabel.replace('{seconds}', String(seconds))
})

function startTicking(): void {
  stopTicking()
  now.value = Date.now()
  tickTimer = setInterval(() => {
    now.value = Date.now()
  }, 500)
}

function stopTicking(): void {
  if (tickTimer) {
    clearInterval(tickTimer)
    tickTimer = null
  }
}

function closeCurrent(): void {
  const e = entry.value
  if (!e) return
  closeGlobalDialog(e._id)
}

function onBackdrop(): void {
  const e = entry.value
  if (!e || e.closable === false || e.closeOnBackdrop === false) return
  closeGlobalDialog(e._id)
}

function onKeydown(ev: KeyboardEvent): void {
  if (ev.key !== 'Escape') return
  const e = entry.value
  if (!e || e.closable === false) return
  closeGlobalDialog(e._id)
}

async function onAction(action: PluginDialogAction): Promise<void> {
  try {
    await action.onClick?.()
  } catch (err) {
    console.error('[PluginDialog] action failed, dialog kept open:', err)
    return
  }
  closeCurrent()
  if (action.navigateTo) {
    try {
      getRouter()?.push(action.navigateTo)
    } catch (err) {
      console.warn('[PluginDialog] navigate failed:', err)
    }
  }
}

const ContentShell = defineComponent({
  name: 'PluginDialogContentShell',
  props: {
    entry: { type: Object, required: true },
  },
  setup(props) {
    provide('pluginContext', (props.entry as GlobalDialogEntry).pluginContext)
    const content = () =>
      h(
        (props.entry as GlobalDialogEntry).content,
        { ...(props.entry as GlobalDialogEntry).props, onClose: () => closeCurrent() },
      )
    return content
  },
})

onMounted(() => {
  unsubscribe = subscribeGlobalDialog((e) => {
    entry.value = e
    if (e) startTicking()
    else stopTicking()
  })
  window.addEventListener('keydown', onKeydown)
})

watch(
  () => entry.value?._id,
  () => {
    now.value = Date.now()
  },
)

onBeforeUnmount(() => {
  unsubscribe?.()
  unsubscribe = null
  stopTicking()
  window.removeEventListener('keydown', onKeydown)
})
</script>

<style scoped>
.pgd-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  padding-bottom: calc(24px + var(--safe-area-bottom, 0px));
  background: var(--mobile-overlay-heavy, rgba(0, 0, 0, 0.5));
  backdrop-filter: blur(4px);
}

.pgd-card {
  position: relative;
  width: 100%;
  display: flex;
  flex-direction: column;
  max-height: 85vh;
  overflow: auto;
  background: var(--mobile-bg-card);
  border: 1px solid var(--mobile-border);
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
}

.pgd-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}

/* 有关闭按钮（closable）时给标题行右侧让位：绝对定位的 .pgd-close 占右上角
   （top 12px + 右 12px + 28px 宽），否则倒计时/长标题会与 X 重叠 */
.pgd-head--closable {
  padding-right: 44px;
}

.pgd-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--mobile-text-primary);
  line-height: 1.4;
}

.pgd-countdown {
  font-size: 12px;
  color: var(--mobile-text-secondary);
  flex-shrink: 0;
  white-space: nowrap;
  transition: color 0.2s ease;
  font-variant-numeric: tabular-nums;
}

.pgd-countdown--urgent {
  color: var(--mobile-danger-color, #ef4444);
  font-weight: 600;
}

.pgd-message {
  margin: 0;
  font-size: 14px;
  line-height: 1.55;
  color: var(--mobile-text-secondary);
  overflow-wrap: anywhere;
}

.pgd-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}

.pgd-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 44px;
  padding: 8px 16px;
  background: var(--mobile-bg-primary, #f3f4f6);
  color: var(--mobile-text-primary);
  border: 1px solid var(--mobile-border);
  border-radius: 12px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color 0.2s ease, background-color 0.2s ease, opacity 0.2s ease;
  flex-shrink: 0;
  flex: 1;
}

.pgd-btn:hover {
  border-color: var(--mobile-accent);
}

.pgd-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  border-color: var(--mobile-border);
  background: var(--mobile-bg-primary);
  color: var(--mobile-text-secondary);
}

.pgd-btn--primary {
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
  color: var(--mobile-text-on-accent, #fff);
}

.pgd-btn--primary:hover {
  filter: brightness(0.95);
}

.pgd-btn--danger {
  background: var(--mobile-danger-color, #ef4444);
  border-color: var(--mobile-danger-color, #ef4444);
  color: #fff;
}

.pgd-btn--ghost {
  border-color: transparent;
  background: transparent;
  opacity: 0.65;
}

.pgd-close {
  position: absolute;
  top: 12px;
  right: 12px;
  padding: 6px;
  border: none;
  background: transparent;
  color: var(--mobile-text-muted);
  border-radius: 8px;
  cursor: pointer;
  transition: color 0.2s ease, background-color 0.2s ease;
}

.pgd-close:hover {
  color: var(--mobile-text-primary);
  background: var(--mobile-bg-primary);
}

.pgd-close-ico {
  width: 16px;
  height: 16px;
  display: block;
}

.pgd-enter-active,
.pgd-leave-active {
  transition: opacity 0.2s ease;
}

.pgd-enter-active .pgd-card,
.pgd-leave-active .pgd-card {
  transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.pgd-enter-from,
.pgd-leave-to {
  opacity: 0;
}

.pgd-enter-from .pgd-card,
.pgd-leave-to .pgd-card {
  transform: translateY(8px) scale(0.98);
}
</style>
