<template>
  <SettingsSubPage :title="$t('peers.transfers.title')">
    <div class="px-4 py-4 space-y-4">
      <!-- ==================== 发送表单：选源 → 选设备 → 扇出发送 ==================== -->
      <div class="settings-group p-4 space-y-3">
        <!-- 源选择：Android 无目录选择能力，隐藏文件夹入口 -->
        <div class="flex items-center gap-2 flex-wrap">
          <button
            class="min-h-[44px] px-4 rounded-xl text-sm font-medium transition-colors duration-200 active:opacity-80"
            style="color: var(--mobile-text-primary); background: color-mix(in srgb, var(--mobile-accent) 16%, transparent)"
            @click="handlePickFiles"
          >
            {{ $t('peers.transfers.pickFiles') }}
          </button>
          <button
            v-if="!isAndroid"
            class="min-h-[44px] px-4 rounded-xl text-sm font-medium transition-colors duration-200 active:opacity-80"
            style="color: var(--mobile-text-primary); background: color-mix(in srgb, var(--mobile-accent) 16%, transparent)"
            @click="handlePickFolder"
          >
            {{ $t('peers.transfers.pickFolder') }}
          </button>
        </div>

        <p
          v-if="selectedPaths.length > 0"
          class="text-xs truncate"
          style="color: var(--mobile-row-sub)"
        >
          {{ $t('peers.transfers.selectedCount', { count: selectedPaths.length }) }}
        </p>
        <p v-else class="text-xs" style="color: var(--mobile-row-sub)">
          {{ $t('peers.transfers.selectedEmpty') }}
        </p>

        <!-- 已选清单（chips，可逐项移除；移除热区 44px 触达） -->
        <div v-if="selectedPaths.length > 0" class="flex flex-wrap gap-1.5">
          <span
            v-for="(path, index) in selectedPaths"
            :key="`${path}-${index}`"
            class="inline-flex items-center gap-1 pl-2 pr-0 h-11 rounded-xl max-w-[240px] text-sm"
            style="border: 1px solid var(--mobile-border); color: var(--mobile-text-secondary); background: color-mix(in srgb, var(--mobile-bg-tertiary) 60%, transparent)"
          >
            <span class="truncate">{{ fileNameOf(path) }}</span>
            <button
              class="w-11 h-11 flex-shrink-0 flex items-center justify-center active:opacity-70 transition-opacity duration-200"
              :aria-label="$t('peers.transfers.removeItem')"
              @click="removePath(index)"
            >
              <svg class="w-4 h-4" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </span>
        </div>

        <!-- 目标设备多选：具备传输能力的发现节点，点击切换选中（自绘勾选，无原生控件） -->
        <div class="space-y-2">
          <p class="text-xs" style="color: var(--mobile-row-sub)">
            {{ capablePeers.length === 0 ? $t('peers.transfers.noCapablePeers') : $t('peers.transfers.peersSection') }}
          </p>
          <button
            v-for="peer in capablePeers"
            :key="peer.nodeId"
            class="w-full min-h-[44px] flex items-center gap-3 px-3 py-2.5 rounded-xl border text-left transition-colors duration-200 active:opacity-80"
            :style="
              isSelected(peer.nodeId)
                ? 'border-color: var(--mobile-accent); background: color-mix(in srgb, var(--mobile-accent) 10%, transparent)'
                : 'border-color: var(--mobile-border)'
            "
            @click="togglePeer(peer.nodeId)"
          >
            <span
              class="w-[18px] h-[18px] rounded-md border flex-shrink-0 flex items-center justify-center transition-colors duration-200"
              :style="
                isSelected(peer.nodeId)
                  ? 'background: var(--mobile-accent); border-color: transparent'
                  : 'border-color: var(--mobile-border)'
              "
            >
              <svg
                v-if="isSelected(peer.nodeId)"
                class="w-3 h-3"
                style="color: var(--mobile-text-on-accent)"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
            </span>
            <span class="flex-1 min-w-0">
              <span class="block text-sm font-medium text-[var(--mobile-text-primary)] truncate">
                {{ peer.deviceName }}
              </span>
              <span class="block text-xs truncate" style="color: var(--mobile-row-sub)">
                {{ shortFingerprint(peer.nodeId) }} · {{ peer.addr }}
              </span>
            </span>
          </button>
          <p
            v-if="selectedPeerIds.length > 1"
            class="text-xs leading-relaxed"
            style="color: var(--mobile-row-sub)"
          >
            {{ $t('peers.transfers.fanoutHint') }}
          </p>
        </div>

        <!-- 扇出发起失败反馈（失败台数如实提示，不影响已成功发起的批次） -->
        <p
          v-if="failedNodeIds.length > 0"
          class="text-xs"
          style="color: var(--mobile-error)"
        >
          {{ failedNodeIds.join(', ') }}
        </p>

        <button
          class="w-full min-h-[44px] rounded-xl text-sm font-semibold transition-opacity duration-200 active:opacity-80 disabled:opacity-40"
          style="color: var(--mobile-text-on-accent); background: var(--mobile-accent)"
          :disabled="!canSend || sending"
          @click="handleSend"
        >
          {{ sending ? $t('peers.transfers.sending') : $t('peers.transfers.sendTo', { count: selectedPeerIds.length }) }}
        </button>
      </div>

      <!-- ==================== 正在接收（issue 10）：pending 待应答 + running 进度/取消 ==================== -->
      <div v-if="activeReceiving.length > 0" class="space-y-2">
        <h3 class="text-sm font-semibold text-[var(--mobile-text-primary)] px-1">
          {{ $t('peers.transfers.receivingSection') }}
        </h3>
        <div
          v-for="task in activeReceiving"
          :key="task.batchId"
          class="settings-group p-4 space-y-2"
        >
          <div class="flex items-center gap-3 min-w-0">
            <!-- 状态点：pending 待确认琥珀色，running 主色 -->
            <span
              class="w-2 h-2 rounded-full flex-shrink-0"
              :style="{ background: task.status === 'pending' ? 'var(--mobile-warning)' : 'var(--mobile-accent)' }"
            ></span>
            <span class="flex-1 min-w-0 text-sm font-medium text-[var(--mobile-text-primary)] truncate">
              {{ task.peerName }}
            </span>
            <span class="text-xs flex-shrink-0" style="color: var(--mobile-row-sub)">
              {{ $t(`peers.transfers.status.${task.status}`) }}
            </span>
            <!-- 44px 触达的取消按钮 -->
            <button
              class="flex-shrink-0 min-h-[44px] px-3 rounded-xl text-sm font-medium active:opacity-80 transition-opacity duration-200"
              style="color: var(--mobile-error); background: color-mix(in srgb, var(--mobile-error) 12%, transparent)"
              @click="handleCancelReceiving(task.batchId)"
            >
              {{ $t('peers.transfers.cancel') }}
            </button>
          </div>
          <p v-if="task.files.length > 0" class="text-xs truncate" style="color: var(--mobile-row-sub)">
            {{ $t('peers.transfers.filesCount', { count: task.files.length }) }} ·
            {{ formatBytes(task.totalBytes) }}
          </p>
          <!-- pending 行展示倒计时；running 行展示进度条 -->
          <p v-if="task.status === 'pending'" class="text-xs" style="color: var(--mobile-row-sub)">
            {{ $t('peers.transfers.countdown', { seconds: remainingSecsFor(task) }) }}
          </p>
          <template v-else>
            <div class="h-1.5 rounded-full overflow-hidden" style="background: color-mix(in srgb, var(--mobile-row-sub) 20%, transparent)">
              <div
                class="w-full h-full rounded-full origin-left transition-transform duration-200"
                :style="{ transform: `scaleX(${progressPercent(task) / 100})`, background: 'var(--mobile-success)' }"
              ></div>
            </div>
            <div class="flex items-center gap-2 text-xs" style="color: var(--mobile-row-sub)">
              <span>{{ progressPercent(task) }}%</span>
              <span class="truncate">
                {{ formatBytes(task.transferredBytes) }} / {{ formatBytes(task.totalBytes) }}
              </span>
              <span class="ml-auto flex-shrink-0">{{ formatRate(task.rateBps) }}</span>
            </div>
          </template>
        </div>
      </div>

      <!-- ==================== 正在发送：实时进度 / 速率 / 取消 ==================== -->
      <div v-if="activeTransfers.length > 0" class="space-y-2">
        <h3 class="text-sm font-semibold text-[var(--mobile-text-primary)] px-1">
          {{ $t('peers.transfers.sendingSection') }}
        </h3>
        <div
          v-for="task in activeTransfers"
          :key="task.batchId"
          class="settings-group p-4 space-y-2"
        >
          <div class="flex items-center gap-3 min-w-0">
            <span class="flex-1 min-w-0 text-sm font-medium text-[var(--mobile-text-primary)] truncate">
              {{ task.peerName }}
            </span>
            <!-- 44px 触达的取消按钮 -->
            <button
              class="flex-shrink-0 min-h-[44px] px-3 rounded-xl text-sm font-medium active:opacity-80 transition-opacity duration-200"
              style="color: var(--mobile-error); background: color-mix(in srgb, var(--mobile-error) 12%, transparent)"
              @click="cancel(task.batchId)"
            >
              {{ $t('peers.transfers.cancel') }}
            </button>
          </div>
          <!-- 进度条：批累计口径（含续传基线），速率实时；scaleX 动画走合成层 -->
          <div class="h-1.5 rounded-full overflow-hidden" style="background: color-mix(in srgb, var(--mobile-row-sub) 20%, transparent)">
            <div
              class="w-full h-full rounded-full origin-left transition-transform duration-200"
              :style="{ transform: `scaleX(${progressPercent(task) / 100})`, background: 'var(--mobile-accent)' }"
            ></div>
          </div>
          <div class="flex items-center gap-2 text-xs" style="color: var(--mobile-row-sub)">
            <span>{{ progressPercent(task) }}%</span>
            <span class="truncate">
              {{ formatBytes(task.transferredBytes) }} / {{ formatBytes(task.totalBytes) }}
            </span>
            <span class="ml-auto flex-shrink-0">{{ formatRate(task.rateBps) }}</span>
          </div>
        </div>
      </div>

      <!-- ==================== 历史：全部终态（发送 + 接收），可清空 ==================== -->
      <div v-if="mergedHistory.length > 0" class="space-y-2">
        <div class="flex items-center justify-between px-1">
          <h3 class="text-sm font-semibold text-[var(--mobile-text-primary)]">
            {{ $t('peers.transfers.historyTitle') }}
          </h3>
          <button
            class="min-h-[44px] px-3 text-sm font-medium active:opacity-80 transition-opacity duration-200"
            style="color: var(--mobile-error)"
            @click="handleClearHistory"
          >
            {{ $t('peers.transfers.clearHistory') }}
          </button>
        </div>
        <div
          v-for="task in mergedHistory"
          :key="task.batchId"
          class="settings-group p-4"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 min-w-0">
                <!-- 终态色点 -->
                <span
                  class="w-2 h-2 rounded-full flex-shrink-0"
                  :style="{ background: statusColor(task.status) }"
                ></span>
                <!-- 方向徽标：同页混合双方向时一眼可辨 -->
                <span
                  class="flex-shrink-0 px-1.5 py-0.5 rounded text-xs font-medium"
                  :style="
                    task.direction === 'receive'
                      ? 'color: var(--mobile-success); background: color-mix(in srgb, var(--mobile-success) 12%, transparent)'
                      : 'color: var(--mobile-row-sub); background: color-mix(in srgb, var(--mobile-row-sub) 12%, transparent)'
                  "
                >
                  {{ $t(task.direction === 'receive' ? 'peers.transfers.receivingSection' : 'peers.transfers.sendingSection') }}
                </span>
                <span class="text-sm font-medium text-[var(--mobile-text-primary)] truncate">
                  {{ task.peerName }}
                </span>
                <span class="text-xs flex-shrink-0" :style="{ color: statusColor(task.status) }">
                  {{ statusLabel(task) }}
                </span>
              </div>
              <p class="text-xs mt-1 truncate" style="color: var(--mobile-row-sub)">
                {{ $t('peers.transfers.filesCount', { count: task.files.length }) }} ·
                {{ formatBytes(task.totalBytes) }} ·
                {{ new Date(task.updatedAtMs).toLocaleString() }}
              </p>
              <p v-if="task.rejectReason" class="text-xs mt-0.5" style="color: var(--mobile-row-sub)">
                {{ $t(`peers.transfers.rejectReason.${task.rejectReason}`) }}
              </p>
              <p
                v-if="retryFailedIds.has(task.batchId)"
                class="text-xs mt-0.5"
                style="color: var(--mobile-error)"
              >
                {{ $t('peers.transfers.retryFailed') }}
              </p>
            </div>
            <!-- 重试入口：断点续传从接收端已写偏移继续 -->
            <button
              v-if="task.status !== 'completed'"
              class="flex-shrink-0 min-h-[44px] px-4 rounded-xl text-sm font-medium active:opacity-80 transition-opacity duration-200"
              style="color: var(--mobile-text-on-accent); background: var(--mobile-accent)"
              @click="handleRetry(task.batchId)"
            >
              {{ $t('peers.transfers.retry') }}
            </button>
          </div>
        </div>
      </div>

      <!-- 全空态：发送与接收均无任何记录 -->
      <div v-if="transfers.length === 0 && receivingTasks.length === 0" class="settings-group py-8 text-center">
        <p class="text-sm text-[var(--mobile-text-muted)]">{{ $t('peers.transfers.empty') }}</p>
      </div>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 对等网络传输任务二级页 — 移动端发送侧全流程入口（issue 09）
 *
 * 表单（选源 + 多选目标设备）→ 扇出发送（每台一条独立批）；进行中区实时
 * 进度/速率/取消；历史区承载全部终态并可重试走断点续传。列表由宿主事件
 * 驱动自动刷新。目录发送仅桌面端提供——Android SAF 目录树 URI 无法直接
 * 作发送源，按钮按平台隐藏。
 */
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import {
  usePeerTransfers,
  type PeerTransferTask,
} from '@/composables/usePeerTransfers'
import { usePeerReceiving, type PeerReceiveTask } from '@/composables/usePeerReceiving'
import { usePeerDevices } from '@/composables/usePeerDevices'
import { usePlatform } from '@/composables/usePlatform'

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
const { platformInfo, detectPlatform } = usePlatform()

const isAndroid = computed(() => platformInfo.value.isAndroid)

// ==================== 发送表单状态 ====================

/** 已选源路径（宿主侧解析真实路径并递归展开目录） */
const selectedPaths = ref<string[]>([])
const selectedPeerIds = ref<string[]>([])
const sending = ref(false)
const failedNodeIds = ref<string[]>([])
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
    // 失败的节点保留在选中集以便重发；全部成功时清空表单
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
  const next = new Set(retryFailedIds.value)
  if (ok) {
    next.delete(batchId)
  } else {
    next.add(batchId)
  }
  retryFailedIds.value = next
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

function statusLabel(task: PeerTransferTask): string {
  return t(`peers.transfers.status.${task.status}`)
}

function statusColor(status: PeerTransferTask['status']): string {
  switch (status) {
    case 'completed':
      return 'var(--mobile-success)'
    case 'failed':
      return 'var(--mobile-error)'
    case 'rejected':
      return 'var(--mobile-error)'
    case 'cancelled':
      return 'var(--mobile-warning)'
    default:
      return 'var(--mobile-row-sub)'
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

onMounted(async () => {
  await detectPlatform()
  void start()
  // 设备列表数据源复用 issue 08 的单例编排：页面晚启动也能拿到快照
  void startDevices()
  // 接收侧单例（issue 10）：拉取接收快照 + 启动询问倒计时心跳
  void startReceiving()
})
</script>
