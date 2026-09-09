<script setup lang="ts">
/**
 * BatchRequestDialog — 全局批量传输请求弹窗
 *
 * 交互契约（spec 14.4）：
 * - 多个 pending 批排队逐个提示（按创建时间升序 = 先到先弹）
 * - 必须明确选择「接受全部 / 拒绝全部」（无背景关闭、无关闭按钮）
 * - 倒计时归零自动关闭，默认拒绝由宿主 pending TTL（reason=timeout）执行
 * - 批被 resolved → 关闭并提示下一批
 * 视觉：分段按钮 + 倒计时进度条（替代旧版纯按钮排版）。
 */
import { inject, onUnmounted, ref, watch, computed } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
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

/** 已提示过的批 ID（防重复弹框；批 resolved / 应答 / 超时后不再提示） */
const promptedBatches = new Set<string>()
/** 当前弹窗展示的批（null = 无待提示批，不渲染） */
const current = ref<PendingBatch | null>(null)
/** 剩余应答秒数（归零自动关闭） */
const secondsLeft = ref(0)

/** 倒计时进度百分比（相对配置超时） */
const countdownPercent = computed(() =>
  props.approvalTimeoutSec > 0
    ? Math.min(100, Math.max(0, (secondsLeft.value / props.approvalTimeoutSec) * 100))
    : 0,
)

let timer: ReturnType<typeof setInterval> | null = null

/** 取第一个未提示的 pending 批（按创建时间升序 = 先到先弹） */
function nextUnprompted(): PendingBatch | null {
  const candidates = props.batches
    .filter((b) => !promptedBatches.has(b.batchId))
    .sort((a, b) => a.createdAt - b.createdAt)
  return candidates[0] ?? null
}

function stopTimer(): void {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

/** 启动倒计时（基于批创建时间 + 配置超时，与宿主 TTL 对齐） */
function startCountdown(batch: PendingBatch): void {
  const deadline = batch.createdAt + props.approvalTimeoutSec * 1000
  const tick = () => {
    secondsLeft.value = Math.max(0, Math.ceil((deadline - Date.now()) / 1000))
    if (secondsLeft.value <= 0) {
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

/** 批列表变化：当前批被 resolved → 关闭并提示下一批 */
watch(
  () => props.batches,
  (batches) => {
    const currentId = current.value?.batchId
    if (currentId && !batches.some((b) => b.batchId === currentId)) {
      stopTimer()
      advance()
    } else if (!current.value) {
      advance()
    }
  },
  { deep: true },
)

onUnmounted(stopTimer)

/** 文件列表展示（最多 3 行，超出折叠为「+N」） */
const visibleFiles = computed(() => (current.value?.files ?? []).slice(0, 3))
const hiddenFileCount = computed(
  () => Math.max(0, (current.value?.files.length ?? 0) - 3),
)

/** 文件名 basename */
function basename(path: string): string {
  return path.split('/').pop() || path
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fv2-fade">
      <div v-if="current" class="fixed inset-0 z-[100] flex items-center justify-center mobile-ui px-6">
        <!-- Backdrop：不可点关闭（必须明确应答） -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]"></div>

        <!-- 面板 -->
        <div
          class="fv2-sheet-panel relative w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-4"
          style="max-width: 26rem"
        >
          <!-- 标题行 -->
          <div class="flex items-center gap-3">
            <span class="icon-chip chip-cyan flex-shrink-0">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
              </svg>
            </span>
            <div class="flex-1 min-w-0">
              <p class="group-row-title">{{ t('transfer.request.title') }}</p>
              <p class="group-row-sub mt-0.5 truncate">{{ current.peerName }}</p>
            </div>
          </div>

          <!-- 请求摘要 -->
          <p class="group-row-sub mt-3" style="line-height: 1.5; color: var(--mobile-text-secondary)">
            {{
              t('transfer.request.body', {
                name: current.peerName,
                count: current.files.length,
                size: formatBytes(current.totalSize, t),
              })
            }}
          </p>

          <!-- 文件清单（最多 3 行 + 折叠计数） -->
          <div class="group-card mt-3">
            <div
              v-for="(file, i) in visibleFiles"
              :key="file.relativePath"
              class="group-row"
              :style="i > 0 ? { borderTop: '1px solid var(--mobile-group-divider)' } : undefined"
              style="min-height: 2.5rem; padding-top: 0.5rem; padding-bottom: 0.5rem"
            >
              <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
              <span class="flex-1 min-w-0 group-row-sub truncate">{{ basename(file.relativePath) }}</span>
              <span class="group-row-sub flex-shrink-0" style="font-variant-numeric: tabular-nums">
                {{ formatBytes(file.size, t) }}
              </span>
            </div>
          </div>

          <!-- 倒计时：秒数 + 进度条（时间流逝视觉） -->
          <div class="mt-3">
            <div class="flex items-center justify-between">
              <span class="group-row-sub" style="color: var(--mobile-warning)">
                {{ t('transfer.request.countdown', { seconds: secondsLeft }) }}
              </span>
              <span class="group-row-sub" style="font-variant-numeric: tabular-nums; color: var(--mobile-warning)">
                {{ secondsLeft }}s
              </span>
            </div>
            <div class="fv2-countdown mt-1.5">
              <div class="fv2-countdown-fill" :style="{ width: countdownPercent + '%' }"></div>
            </div>
          </div>

          <!-- 折叠文件数提示 -->
          <p v-if="hiddenFileCount > 0" class="group-row-sub mt-2" style="color: var(--mobile-text-muted)">
            +{{ hiddenFileCount }}
          </p>

          <!-- 应答按钮：必须明确选择 -->
          <div class="flex gap-3 mt-4">
            <button class="fv2-approve-btn fv2-approve-btn--reject flex-1" @click="handleReject">
              <svg class="w-5 h-5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
              {{ t('transfer.request.rejectAll') }}
            </button>
            <button class="fv2-approve-btn fv2-approve-btn--accept flex-1" @click="handleApprove">
              <svg class="w-5 h-5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
              </svg>
              {{ t('transfer.request.acceptAll') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
