<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3 min-w-0">
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('peers.transfers.title') }}
        </h2>
        <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
          {{ t('peers.transfers.subtitle') }}
        </span>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-5">
      <div class="space-y-6 max-w-3xl">
        <!-- ==================== 发送表单：选源 → 选设备 → 扇出发送 ==================== -->
        <section class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-4 space-y-4">
          <!-- 源文件选择 -->
          <div class="flex items-center gap-2 flex-wrap">
            <button class="wb-btn-ghost" @click="handlePickFiles">
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
              </svg>
              {{ t('peers.transfers.pickFiles') }}
            </button>
            <button class="wb-btn-ghost" @click="handlePickFolder">
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
              </svg>
              {{ t('peers.transfers.pickFolder') }}
            </button>
            <span
              v-if="selectedPaths.length > 0"
              class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)]"
            >
              {{ t('peers.transfers.selectedCount', { count: selectedPaths.length }) }}
            </span>
            <span v-else class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
              {{ t('peers.transfers.selectedEmpty') }}
            </span>
          </div>

          <!-- 已选清单（chips，可逐项移除） -->
          <div v-if="selectedPaths.length > 0" class="flex flex-wrap gap-1.5">
            <span
              v-for="(path, index) in selectedPaths"
              :key="`${path}-${index}`"
              class="inline-flex items-center gap-1 pl-2 pr-1 h-6 rounded-md border border-[var(--border)] bg-[var(--bg-hover)] text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] max-w-[260px]"
            >
              <span class="truncate">{{ fileNameOf(path) }}</span>
              <button
                class="w-4 h-4 rounded flex-shrink-0 flex items-center justify-center text-[var(--text-tertiary)] hover:text-red-500 dark:hover:text-red-400 transition-colors duration-200"
                :title="t('peers.transfers.removeItem')"
                @click="removePath(index)"
              >
                <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </span>
          </div>

          <!-- 目标设备多选：具备传输能力的发现节点，点击切换选中 -->
          <div class="space-y-2">
            <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
              {{ capablePeers.length === 0 ? t('peers.transfers.noCapablePeers') : t('peers.transfers.peersSection') }}
            </p>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
              <button
                v-for="peer in capablePeers"
                :key="peer.nodeId"
                class="flex items-center gap-3 px-3 h-11 rounded-lg border text-left transition-colors duration-200"
                :class="
                  isSelected(peer.nodeId)
                    ? 'border-[var(--color-primary)] bg-[var(--bg-hover)]'
                    : 'border-[var(--border)] hover:bg-[var(--bg-hover)]'
                "
                :disabled="!peer.fileTransfer"
                @click="togglePeer(peer.nodeId)"
              >
                <!-- 自绘选中标记（禁用原生 checkbox 外观） -->
                <span
                  class="w-4 h-4 rounded border flex-shrink-0 flex items-center justify-center transition-colors duration-200"
                  :class="isSelected(peer.nodeId) ? 'bg-[var(--color-primary)] border-transparent' : 'border-[var(--border)]'"
                >
                  <svg v-if="isSelected(peer.nodeId)" class="w-3 h-3 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
                  </svg>
                </span>
                <span class="flex-1 min-w-0">
                  <span class="block text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">
                    {{ peer.deviceName }}
                  </span>
                  <span class="block text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
                    {{ shortFingerprint(peer.nodeId) }} · {{ peer.addr }}
                  </span>
                </span>
              </button>
            </div>
            <p v-if="selectedPeerIds.length > 1" class="text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)] px-0.5">
              {{ t('peers.transfers.fanoutHint') }}
            </p>
          </div>

          <!-- 发起失败反馈（扇出中单台失败不影响其余，失败台数如实提示） -->
          <p
            v-if="failedNodeIds.length > 0"
            class="text-[calc(11px*var(--ui-scale))] text-red-500 dark:text-red-400"
          >
            {{ failedNodeIds.join(', ') }}
          </p>

          <div class="flex items-center justify-between gap-3">
            <p v-if="selectedPeerIds.length > 0 && selectedPaths.length > 0" class="text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)]">
              {{ t('peers.transfers.selectPeersHint') }}
            </p>
            <span v-else></span>
            <button
              class="wb-btn-primary flex-shrink-0"
              :disabled="!canSend || sending"
              @click="handleSend"
            >
              {{ sending ? t('peers.transfers.sending') : t('peers.transfers.sendTo', { count: selectedPeerIds.length }) }}
            </button>
          </div>
        </section>

        <!-- ==================== 正在接收（issue 10）：pending 待应答 + running 进度/取消 ==================== -->
        <section v-if="activeReceiving.length > 0" class="space-y-2">
          <h3 class="text-[calc(12px*var(--ui-scale))] font-semibold text-[var(--text-secondary)] px-1">
            {{ t('peers.transfers.receivingSection') }}
          </h3>
          <div
            v-for="task in activeReceiving"
            :key="task.batchId"
            class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3 space-y-2"
          >
            <div class="flex items-center gap-3 min-w-0">
              <!-- 状态点：pending 待确认琥珀色，running 主色 -->
              <span
                class="w-2 h-2 rounded-full flex-shrink-0"
                :class="task.status === 'pending' ? 'bg-amber-500' : 'bg-[var(--color-primary)]'"
              ></span>
              <span class="flex-1 min-w-0 text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">
                {{ task.peerName }}
              </span>
              <span
                class="flex-shrink-0 text-[calc(10px*var(--ui-scale))]"
                :class="task.status === 'pending' ? 'text-amber-600 dark:text-amber-400' : 'text-[var(--text-tertiary)]'"
              >
                {{ t(`peers.transfers.status.${task.status}`) }}
              </span>
              <span class="flex-shrink-0 text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)]">
                {{ t('peers.transfers.filesCount', { count: task.files.length }) }} · {{ formatBytes(task.totalBytes) }}
              </span>
              <button class="wb-btn-ghost flex-shrink-0" @click="handleCancelReceiving(task.batchId)">
                {{ t('peers.transfers.cancel') }}
              </button>
            </div>
            <!-- pending 行展示倒计时；running 行展示进度条 -->
            <p
              v-if="task.status === 'pending'"
              class="text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)]"
            >
              {{ t('peers.transfers.countdown', { seconds: remainingSecsFor(task) }) }}
            </p>
            <template v-else>
              <div class="h-1.5 rounded-full bg-[var(--bg-hover)] overflow-hidden">
                <div
                  class="w-full h-full rounded-full bg-green-500 origin-left transition-transform duration-200"
                  :style="{ transform: `scaleX(${progressPercent(task) / 100})` }"
                ></div>
              </div>
              <div class="flex items-center gap-2 text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)]">
                <span>{{ progressPercent(task) }}%</span>
                <span class="truncate">{{ formatBytes(task.transferredBytes) }} / {{ formatBytes(task.totalBytes) }}</span>
                <span class="ml-auto flex-shrink-0">{{ formatRate(task.rateBps) }}</span>
              </div>
            </template>
          </div>
        </section>

        <!-- ==================== 正在发送：实时进度 / 速率 / 取消 ==================== -->
        <section v-if="activeTransfers.length > 0" class="space-y-2">
          <h3 class="text-[calc(12px*var(--ui-scale))] font-semibold text-[var(--text-secondary)] px-1">
            {{ t('peers.transfers.sendingSection') }}
          </h3>
          <div
            v-for="task in activeTransfers"
            :key="task.batchId"
            class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3 space-y-2"
          >
            <div class="flex items-center gap-3 min-w-0">
              <span class="flex-1 min-w-0 text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">
                {{ task.peerName }}
              </span>
              <span class="flex-shrink-0 text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)]">
                {{ t('peers.transfers.filesCount', { count: task.files.length }) }} · {{ formatBytes(task.totalBytes) }}
              </span>
              <button class="wb-btn-ghost flex-shrink-0" @click="cancel(task.batchId)">
                {{ t('peers.transfers.cancel') }}
              </button>
            </div>
            <!-- 进度条：批累计口径（含续传基线），速率实时；scaleX 动画走合成层 -->
            <div class="h-1.5 rounded-full bg-[var(--bg-hover)] overflow-hidden">
              <div
                class="w-full h-full rounded-full bg-[var(--color-primary)] origin-left transition-transform duration-200"
                :style="{ transform: `scaleX(${progressPercent(task) / 100})` }"
              ></div>
            </div>
            <div class="flex items-center gap-2 text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)]">
              <span>{{ progressPercent(task) }}%</span>
              <span class="truncate">{{ formatBytes(task.transferredBytes) }} / {{ formatBytes(task.totalBytes) }}</span>
              <span class="ml-auto flex-shrink-0">{{ formatRate(task.rateBps) }}</span>
            </div>
          </div>
        </section>

        <!-- ==================== 历史：全部终态（发送 + 接收），可清空 ==================== -->
        <section v-if="mergedHistory.length > 0" class="space-y-2">
          <div class="flex items-center justify-between px-1">
            <h3 class="text-[calc(12px*var(--ui-scale))] font-semibold text-[var(--text-secondary)]">
              {{ t('peers.transfers.historyTitle') }}
            </h3>
            <button class="wb-btn-ghost" @click="handleClearHistory">
              {{ t('peers.transfers.clearHistory') }}
            </button>
          </div>
          <div
            v-for="task in mergedHistory"
            :key="task.batchId"
            class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3 flex items-center gap-3"
          >
            <!-- 终态色点 -->
            <span
              class="w-2 h-2 rounded-full flex-shrink-0"
              :class="statusDotClass(task.status)"
            ></span>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 min-w-0">
                <!-- 方向标记：同页混合双方向时一眼可辨 -->
                <span
                  class="flex-shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium"
                  :class="
                    task.direction === 'receive'
                      ? 'bg-green-500/10 text-green-600 dark:text-green-400'
                      : 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
                  "
                >
                  {{ t(task.direction === 'receive' ? 'peers.transfers.receivingSection' : 'peers.transfers.sendingSection') }}
                </span>
                <span class="text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">
                  {{ task.peerName }}
                </span>
                <span
                  class="flex-shrink-0 text-[calc(10px*var(--ui-scale))]"
                  :class="statusTextClass(task.status)"
                >
                  {{ statusLabel(task) }}
                </span>
              </div>
              <p class="mt-0.5 text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
                {{ t('peers.transfers.filesCount', { count: task.files.length }) }} ·
                {{ formatBytes(task.totalBytes) }} ·
                {{ new Date(task.updatedAtMs).toLocaleString() }}
                <template v-if="task.rejectReason">
                  · {{ t(`peers.transfers.rejectReason.${task.rejectReason}`) }}
                </template>
              </p>
              <p
                v-if="retryFailedIds.has(task.batchId)"
                class="mt-0.5 text-[calc(10px*var(--ui-scale))] text-red-500 dark:text-red-400"
              >
                {{ t('peers.transfers.retryFailed') }}
              </p>
            </div>
            <button
              v-if="task.status !== 'completed'"
              class="wb-btn-ghost flex-shrink-0"
              @click="handleRetry(task.batchId)"
            >
              {{ t('peers.transfers.retry') }}
            </button>
          </div>
        </section>

        <!-- 全空态：发送与接收均无任何记录 -->
        <div
          v-if="transfers.length === 0 && receivingTasks.length === 0"
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-10 text-center"
        >
          <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">
            {{ t('peers.transfers.empty') }}
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 对等网络传输任务页 — 发送侧全流程入口（issue 09）
 *
 * 表单（选源 + 多选目标设备）→ 扇出发送（每台一条独立批，成败互不影响）；
 * 进行中区展示实时进度/速率并支持取消；历史区承载全部终态（封顶滚动、
 * 可清空、失败/取消/被拒可重试走断点续传）。列表由宿主事件驱动自动刷新。
 */
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import {
  usePeerTransfers,
  type PeerTransferTask,
} from '@/composables/usePeerTransfers'
import { usePeerReceiving, type PeerReceiveTask } from '@/composables/usePeerReceiving'
import { usePeerDevices } from '@/composables/usePeerDevices'

const { t } = useI18n()
const { transfers, activeTransfers, historyTransfers, start, sendToPeers, cancel, retry, clearHistory } =
  usePeerTransfers()
const {
  receivingTasks,
  remainingSecs,
  currentOffer,
  settings: receiveSettings,
  start: startReceiving,
  cancel: cancelReceiving,
  clearHistory: clearReceivingHistory,
} = usePeerReceiving()
const { peers, start: startDevices } = usePeerDevices()

// ==================== 发送表单状态 ====================

/** 已选源路径（文件/文件夹混合；宿主侧递归展开目录） */
const selectedPaths = ref<string[]>([])
/** 已选目标节点 ID */
const selectedPeerIds = ref<string[]>([])
const sending = ref(false)
/** 扇出中发起失败的节点（对端离线等；其余设备不受影响） */
const failedNodeIds = ref<string[]>([])
/** 重试失败的批次（源丢失/对端离线时如实反馈） */
const retryFailedIds = ref(new Set<string>())

const capablePeers = computed(() => peers.value.filter((peer) => peer.fileTransfer))

// ==================== 接收侧派生（issue 10：同页分列 + 合并历史） ====================

/** 接收活跃区：pending 待应答 + running 传输中 */
const activeReceiving = computed(() =>
  receivingTasks.value.filter((t) => t.status === 'pending' || t.status === 'running'),
)

/** 接收终态 */
const receivingHistory = computed(() =>
  receivingTasks.value.filter((t) => t.status !== 'pending' && t.status !== 'running'),
)

/** 合并历史（发送 + 接收）：最近更新在前，方向徽标区分来源 */
const mergedHistory = computed(() =>
  [...historyTransfers.value, ...receivingHistory.value].sort(
    (a, b) => b.updatedAtMs - a.updatedAtMs,
  ),
)

/** pending 行倒计时：最早批用心跳值，其余按创建时刻推算静态余量 */
function remainingSecsFor(task: PeerReceiveTask): number {
  const offer = currentOffer.value
  if (offer && offer.batchId === task.batchId) return remainingSecs.value
  return Math.max(0, Math.ceil((task.createdAtMs + receiveSettings.value.askTimeoutSecs * 1000 - Date.now()) / 1000))
}

async function handleCancelReceiving(batchId: string): Promise<void> {
  await cancelReceiving(batchId)
}

const canSend = computed(
  () => selectedPaths.value.length > 0 && selectedPeerIds.value.length > 0,
)

function isSelected(nodeId: string): boolean {
  return selectedPeerIds.value.includes(nodeId)
}

function togglePeer(nodeId: string): void {
  const next = isSelected(nodeId)
    ? selectedPeerIds.value.filter((id) => id !== nodeId)
    : [...selectedPeerIds.value, nodeId]
  selectedPeerIds.value = next
}

function removePath(index: number): void {
  selectedPaths.value = selectedPaths.value.filter((_, i) => i !== index)
}

function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}

async function handlePickFiles(): Promise<void> {
  try {
    const picked = await invoke<string[]>('peer_pick_files')
    selectedPaths.value = mergePaths(selectedPaths.value, picked)
  } catch (error) {
    console.error('[PeerTransfers] pick files failed:', error)
  }
}

async function handlePickFolder(): Promise<void> {
  try {
    const picked = await invoke<string[]>('peer_pick_folder')
    selectedPaths.value = mergePaths(selectedPaths.value, picked)
  } catch (error) {
    // Android 等无目录选择能力的平台：按钮已隐藏，此处防御性兜底
    console.error('[PeerTransfers] pick folder failed:', error)
  }
}

function mergePaths(existing: string[], picked: string[]): string[] {
  const seen = new Set(existing)
  return [...existing, ...picked.filter((p) => !seen.has(p))]
}

async function handleSend(): Promise<void> {
  if (!canSend.value || sending.value) return
  sending.value = true
  failedNodeIds.value = []
  try {
    const paths = [...selectedPaths.value]
    const targets = [...selectedPeerIds.value]
    const outcomes = await sendToPeers(paths, targets)
    // 失败的节点保留在选中集以便重发，成功的保持原样（用户可能继续补发）
    failedNodeIds.value = outcomes.filter((o) => !o.ok).map((o) => o.nodeId)
    if (outcomes.every((o) => o.ok)) {
      selectedPaths.value = []
      selectedPeerIds.value = []
    }
  } finally {
    sending.value = false
  }
}

// ==================== 任务操作 ====================

async function handleRetry(batchId: string): Promise<void> {
  const ok = await retry(batchId)
  if (ok) {
    const next = new Set(retryFailedIds.value)
    next.delete(batchId)
    retryFailedIds.value = next
  } else {
    retryFailedIds.value = new Set(retryFailedIds.value).add(batchId)
  }
}

async function handleClearHistory(): Promise<void> {
  // 双源清空：发送历史（宿主持久化）+ 接收终态（会话内内存）
  await Promise.all([clearHistory(), clearReceivingHistory()])
}

// ==================== 展示辅助 ====================

function shortFingerprint(nodeId: string): string {
  return nodeId.slice(0, 8)
}

function progressPercent(task: PeerTransferTask): number {
  if (task.totalBytes <= 0) return 0
  return Math.min(100, Math.round((task.transferredBytes / task.totalBytes) * 100))
}

function statusLabel(task: PeerTransferTask | PeerReceiveTask): string {
  return t(`peers.transfers.status.${task.status}`)
}

function statusDotClass(status: PeerTransferTask['status'] | 'pending'): string {
  switch (status) {
    case 'completed':
      return 'bg-green-500'
    case 'failed':
      return 'bg-red-500'
    case 'rejected':
      return 'bg-orange-500'
    case 'cancelled':
      return 'bg-yellow-500'
    case 'pending':
      return 'bg-amber-500'
    default:
      return 'bg-[var(--text-tertiary)]'
  }
}

function statusTextClass(status: PeerTransferTask['status'] | 'pending'): string {
  switch (status) {
    case 'completed':
      return 'text-green-600 dark:text-green-400'
    case 'failed':
      return 'text-red-500 dark:text-red-400'
    case 'rejected':
      return 'text-orange-500 dark:text-orange-400'
    case 'cancelled':
      return 'text-yellow-600 dark:text-yellow-400'
    case 'pending':
      return 'text-amber-600 dark:text-amber-400'
    default:
      return 'text-[var(--text-tertiary)]'
  }
}

function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

function formatRate(rateBps: number): string {
  if (!rateBps || rateBps <= 0) return ''
  return `${formatBytes(Math.round(rateBps))}/s`
}

onMounted(() => {
  void start()
  // 设备列表数据源复用 issue 08 的单例编排：页面晚启动也能拿到快照
  void startDevices()
  // 接收侧单例（issue 10）：拉取接收快照 + 启动询问倒计时心跳
  void startReceiving()
})
</script>
