<script setup lang="ts">
/**
 * BatchRequestDialog — 全局批量传输请求弹窗（接收端应答，spec 14.4）
 *
 * 与移动端同构的应答交互：
 * - 多个 pending 批**排队逐个提示**（按创建时间升序 = 先到先弹）
 * - 必须明确选择「接收全部 / 拒绝全部」（无背景关闭、无关闭按钮）
 * - 弹窗显示剩余应答秒数（approvalTimeoutSec 倒计时）；归零**自动关闭**，
 *   默认拒绝由宿主 pending TTL（sweeper，reason=timeout）执行，前端不主动 reject
 * - 批被 resolved（用户从系统通知等处应答 / 宿主超时）→ 自动关闭当前弹窗并提示下一批
 */
import { computed, inject, onUnmounted, ref, watch } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
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
  // 批列表每次由父组件整体重建（引用已变化），无需深度监听
  { deep: true },
)

onUnmounted(stopTimer)
</script>

<template>
  <Teleport to="body">
    <Transition name="ft-dialog">
      <div v-if="current" class="ft-dialog-overlay" role="dialog" aria-modal="true">
        <div class="ft-dialog-card">
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
                name: current.peerName || t('transfer.peer.unknown'),
                count: current.files.length,
                size: formatBytes(current.totalSize),
              })
            }}
          </p>
          <div class="ft-dialog-actions">
            <button class="ft-btn ft-btn--primary" @click="handleApprove">
              {{ t('transfer.request.acceptAll') }}
            </button>
            <button class="ft-btn" @click="handleReject">
              {{ t('transfer.request.rejectAll') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
