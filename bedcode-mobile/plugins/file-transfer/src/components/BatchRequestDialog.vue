<script setup lang="ts">
/**
 * BatchRequestDialog — 全局批量传输请求弹窗（接收端应答，spec 14.4）
 *
 * 与桌面端同构的应答交互：
 * - 多个 pending 批**排队逐个提示**（按创建时间升序 = 先到先弹）
 * - 必须明确选择「接收全部 / 拒绝全部」（无背景关闭、无关闭按钮）
 * - 弹窗显示剩余应答秒数（approvalTimeoutSec 倒计时）；归零**自动关闭**，
 *   默认拒绝由宿主 pending TTL（sweeper，reason=timeout）执行，前端不主动 reject
 * - 批被 resolved（用户从系统通知应答 / 宿主超时）→ 自动关闭当前弹窗并提示下一批
 * - 后台/锁屏场景仍走系统通知（Kotlin TaskNotificationManager），本组件仅前台应答
 */
import { computed, inject, onUnmounted, ref, watch } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import type { PendingBatch } from '../types'
import { formatBytes } from '../utils/format'

const props = defineProps<{
  batches: PendingBatch[]
  approvalTimeoutSec: number
}>()
const emit = defineEmits<{
  approve: [batchId: string]
  reject: [batchId: string]
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

/** 已提示过的批 ID（防同一批重复弹框；批 resolved / 用户应答 / 超时后不再提示） */
const promptedBatches = new Set<string>()
/** 当前弹窗展示的批（null = 无待提示批，不渲染） */
const current = ref<PendingBatch | null>(null)
/** 剩余应答秒数（倒计时；归零自动关闭） */
const secondsLeft = ref(0)

let timer: ReturnType<typeof setInterval> | null = null

/** 取第一个未提示的 pending 批（按创建时间升序 = 先到先弹） */
function nextUnprompted(): PendingBatch | null {
  const candidates = props.batches
    .filter(b => !promptedBatches.has(b.batchId))
    .sort((a, b) => a.createdAt - b.createdAt)
  return candidates[0] ?? null
}

function stopTimer(): void {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

/** 启动当前批的倒计时（基于批创建时间 + 配置超时，与宿主 TTL 对齐） */
function startCountdown(batch: PendingBatch): void {
  const deadline = batch.createdAt + props.approvalTimeoutSec * 1000
  const tick = () => {
    secondsLeft.value = Math.max(0, Math.ceil((deadline - Date.now()) / 1000))
    if (secondsLeft.value <= 0) {
      // 超时：关闭当前弹窗（宿主 pending TTL 拒绝该批，reason=timeout）
      promptedBatches.add(batch.batchId)
      stopTimer()
      advance()
    }
  }
  tick()
  stopTimer()
  timer = setInterval(tick, 1000)
}

/** 弹出下一个未提示的批（无则关闭） */
function advance(): void {
  const next = nextUnprompted()
  if (next) {
    current.value = next
    startCountdown(next)
  } else {
    current.value = null
    stopTimer()
  }
}

/** 用户应答：标记已提示 → 通知宿主 → 弹下一个 */
function handleApprove(): void {
  const batch = current.value
  if (!batch) return
  promptedBatches.add(batch.batchId)
  stopTimer()
  emit('approve', batch.batchId)
  advance()
}

function handleReject(): void {
  const batch = current.value
  if (!batch) return
  promptedBatches.add(batch.batchId)
  stopTimer()
  emit('reject', batch.batchId)
  advance()
}

/** 批列表变化：当前批被 resolved（宿主事件移除）→ 关闭并提示下一批 */
watch(
  () => props.batches,
  (batches) => {
    const currentId = current.value?.batchId
    if (currentId && !batches.some(b => b.batchId === currentId)) {
      stopTimer()
      advance()
    } else if (!current.value) {
      advance()
    }
  },
  { deep: true },
)

onUnmounted(stopTimer)
</script>

<template>
  <Teleport to="body">
    <Transition name="ft-dialog">
      <div v-if="current" class="fixed inset-0 z-[100] flex items-center justify-center px-6 mobile-ui" role="dialog" aria-modal="true">
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]"></div>
        <div class="ft-dialog-card relative w-full max-w-[340px]">
          <div class="ft-dialog-head">
            <span class="ft-dialog-title">{{ t('transfer.request.title') }}</span>
            <span
              class="ft-dialog-countdown"
              :class="{ 'ft-dialog-countdown--urgent': secondsLeft <= 10 }"
            >
              {{ t('transfer.request.countdown', { seconds: secondsLeft }) }}
            </span>
          </div>
          <p class="ft-dialog-body">
            {{
              t('transfer.request.body', {
                name: current.peerName || context.i18n.t('transfer.peer.unknown'),
                count: current.files.length,
                size: formatBytes(current.totalSize, t),
              })
            }}
          </p>
          <div class="ft-dialog-actions">
            <button class="ft-dialog-btn" @click="handleReject">
              {{ t('transfer.request.rejectAll') }}
            </button>
            <button class="ft-dialog-btn ft-dialog-btn--primary" @click="handleApprove">
              {{ t('transfer.request.acceptAll') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* 弹窗卡片：token-bound（--mobile-*），居中模态 */
.ft-dialog-card {
  padding: 1.25rem;
  border-radius: 1rem;
  background: var(--mobile-bg-card);
  border: 1px solid var(--mobile-border);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.ft-dialog-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.75rem;
}

.ft-dialog-title {
  font-size: clamp(0.875rem, 0.9375rem + (100vw - 360px) / 800, 1rem);
  font-weight: 600;
  color: var(--mobile-text-primary);
}

.ft-dialog-countdown {
  flex-shrink: 0;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
  transition: color 0.2s ease;
}

.ft-dialog-countdown--urgent {
  color: var(--mobile-error);
  font-weight: 600;
}

.ft-dialog-body {
  margin: 0;
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  line-height: 1.5;
  color: var(--mobile-text-secondary);
}

.ft-dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.625rem;
  margin-top: 0.25rem;
}

/* 按钮：44px 最小触控目标 */
.ft-dialog-btn {
  min-height: 2.75rem;
  padding: 0 1.125rem;
  border-radius: 0.625rem;
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  font-weight: 500;
  color: var(--mobile-text-primary);
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
}

.ft-dialog-btn:active {
  opacity: 0.8;
}

.ft-dialog-btn--primary {
  color: var(--mobile-text-on-accent);
  background: var(--mobile-accent);
  border-color: var(--mobile-accent);
}

/* 弹窗淡入 + 上浮（GPU 合成属性） */
.ft-dialog-enter-active,
.ft-dialog-leave-active {
  transition: opacity 0.2s ease;
}

.ft-dialog-enter-active .ft-dialog-card,
.ft-dialog-leave-active .ft-dialog-card {
  transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.ft-dialog-enter-from,
.ft-dialog-leave-to {
  opacity: 0;
}

.ft-dialog-enter-from .ft-dialog-card,
.ft-dialog-leave-to .ft-dialog-card {
  transform: translateY(8px) scale(0.98);
}
</style>
