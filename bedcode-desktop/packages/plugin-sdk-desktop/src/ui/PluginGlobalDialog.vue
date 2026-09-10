<template>
  <Teleport to="body">
    <Transition name="pgd">
      <div
        v-if="entry"
        class="pgd-overlay"
        role="dialog"
        aria-modal="true"
        @click.self="onBackdrop"
      >
        <div class="pgd-card" :class="entry.widthClass ?? 'max-w-md'">
          <!-- 预设模式：标题 + 倒计时 + 正文 + 动作按钮 -->
          <template v-if="!entry.content">
            <div class="pgd-head px-5 pt-4">
              <span class="pgd-title">{{ entry.title }}</span>
              <span
                v-if="countdownText"
                class="pgd-countdown"
                :class="{ 'pgd-countdown--urgent': countdownSeconds !== null && countdownSeconds <= 10 }"
              >
                {{ countdownText }}
              </span>
            </div>
            <div class="px-5 py-4">
              <div v-if="entry.message" class="pgd-message">{{ entry.message }}</div>
              <div
                v-else-if="!entry.actions?.length && !countdownText"
                class="pgd-message pgd-message--empty"
              >
                {{ '' }}
              </div>
            </div>
            <div v-if="entry.actions?.length" class="pgd-actions px-5 pb-4">
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
 * PluginGlobalDialog — 宿主全局弹窗（通用能力，桌面宿主 + dev-shell 共用）
 *
 * 两种模式：
 * - 预设模式：title / message / icon / actions，常见确认/拒绝类请求开箱即用；
 * - 组件模式：content 传入插件任意 Vue 组件（provide('pluginContext')），
 *   props 经句柄 update() 热更新。
 *
 * 能力：FIFO 排队（控制器）、可选定时自动关闭 + 倒计时展示（{seconds} 占位文案）、
 * 动作按钮（primary/danger/ghost 变体 + 点击后宿主 router 跳转）、
 * Escape/遮罩/右上角关闭（closable 控制）、z-50 覆盖层 + 令牌化样式。
 *
 * 渲染完全由 getGlobalDialog / subscribeGlobalDialog 驱动——组件不透明地
 * 持有任何业务状态，插件侧经 context.ui.showDialog 操控。
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

/** 当前弹窗（订阅驱动） */
const entry = ref<GlobalDialogEntry | null>(getGlobalDialog())
let unsubscribe: (() => void) | null = null

/** 倒计时展示 = 截止时刻 - 当前时刻；仅在 entry 配置了 timeoutSec / deadlineAt 时非空 */
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
  if (!e?.countdownLabel || seconds == null) return ''
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

/** 关闭当前弹窗（closable 阻止之外的显式关闭路径统一走这里） */
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

/** 预设模式动作点击：onClick 抛错则保持弹窗打开（可重试），成功才关闭并可选跳转 */
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

/**
 * 组件模式外壳：每个条目（:key=_id）新建实例，provide('pluginContext') 供
 * 插件内容组件 inject；内容 emit('close') 关闭弹窗（与预设 actions 同语义）。
 */
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

// ==================== 订阅与全局键盘 ====================

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
    // entry 变化（排队接替）时刷新一次展示起点，countdown 从新条目 deadline 重算
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
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
}

.pgd-card {
  position: relative;
  width: 100%;
  display: flex;
  flex-direction: column;
  max-height: 85vh;
  overflow: auto;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow-card);
}

/* 预设模式内部排版（组件模式由插件内容自带布局） */
.pgd-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}

.pgd-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.pgd-countdown {
  font-size: 12px;
  color: var(--text-secondary);
  flex-shrink: 0;
  white-space: nowrap;
  transition: color 0.2s ease;
  font-variant-numeric: tabular-nums;
}

.pgd-countdown--urgent {
  color: var(--color-danger);
  font-weight: 600;
}

.pgd-message {
  margin: 0;
  font-size: 13px;
  line-height: 1.55;
  color: var(--text-secondary);
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
  gap: 6px;
  padding: 5px 14px;
  background: var(--bg-hover);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius-button);
  font-size: 12px;
  cursor: pointer;
  transition: border-color 0.2s ease, background-color 0.2s ease, opacity 0.2s ease;
  flex-shrink: 0;
}

.pgd-btn:hover {
  border-color: var(--color-primary);
  background: var(--bg-card);
}

.pgd-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  border-color: var(--border);
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.pgd-btn:disabled:hover {
  border-color: var(--border);
  background: var(--bg-hover);
}

.pgd-btn--primary {
  background: var(--color-primary);
  border-color: var(--color-primary);
  color: var(--color-primary-contrast);
}

.pgd-btn--primary:hover {
  background: var(--color-primary-hover);
  border-color: var(--color-primary-hover);
}

.pgd-btn--danger {
  background: var(--color-danger);
  border-color: var(--color-danger);
}

.pgd-btn--danger:hover {
  background: color-mix(in srgb, var(--color-danger) 85%, black);
}

.pgd-btn--ghost {
  border-color: transparent;
  background: transparent;
  opacity: 0.65;
}

.pgd-btn--ghost:hover {
  opacity: 1;
  border-color: color-mix(in srgb, var(--color-danger) 45%, var(--border));
  color: var(--color-danger);
  background: var(--color-danger-light);
}

.pgd-close {
  position: absolute;
  top: 10px;
  right: 10px;
  padding: 4px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  border-radius: 6px;
  cursor: pointer;
  transition: color 0.2s ease, background-color 0.2s ease;
}

.pgd-close:hover {
  color: var(--text-primary);
  background: var(--bg-hover);
}

.pgd-close-ico {
  width: 15px;
  height: 15px;
  display: block;
}

/* 进出场：遮罩淡入 + 卡片上浮（GPU 合成属性） */
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
  transform: translateY(8px);
}
</style>