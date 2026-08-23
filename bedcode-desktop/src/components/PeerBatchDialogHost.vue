<template>
  <Modal
    :model-value="!!offer"
    :title="t('peers.transfers.askTitle')"
    size="sm"
    @close="handleReject"
  >
    <div class="flex flex-col gap-4 py-2">
      <!-- 发送方身份 -->
      <div class="flex items-center gap-3 min-w-0">
        <div
          class="w-10 h-10 rounded-full bg-[var(--color-primary-light)] flex items-center justify-center flex-shrink-0"
        >
          <svg
            class="w-5 h-5 text-[var(--color-primary)]"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M9 17V5a2 2 0 012-2h4a2 2 0 012 2v12m-8 0h8m-8 0a2 2 0 01-2 2H6a2 2 0 01-2-2v-3a2 2 0 012-2h1m10 3h1a2 2 0 002-2v-1"
            />
          </svg>
        </div>
        <div class="min-w-0">
          <p class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">
            {{ t('peers.transfers.fromWithName', { name: offer?.peerName ?? '' }) }}
          </p>
          <p class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
            {{ shortFingerprint }}
          </p>
        </div>
      </div>

      <!-- 文件清单（滚动区；上限保护防超长批撑爆弹窗） -->
      <div class="w-full min-w-0">
        <p class="text-xs text-[var(--text-tertiary)] mb-1.5 flex items-center justify-between">
          <span>{{ t('peers.transfers.fileList') }}</span>
          <span>{{ t('peers.transfers.filesCount', { count: offer?.files.length ?? 0 }) }}</span>
        </p>
        <div
          class="rounded-[8px] border border-[var(--border)] bg-[var(--bg-page)] max-h-48 overflow-y-auto overflow-x-hidden"
        >
          <div
            v-for="(file, index) in offer?.files ?? []"
            :key="index"
            class="px-3 py-1.5 flex items-center justify-between gap-3 border-b border-[var(--border)] last:border-b-0"
          >
            <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] truncate min-w-0">
              {{ file.path }}
            </span>
            <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] flex-shrink-0 wb-mono">
              {{ formatBytes(file.size) }}
            </span>
          </div>
        </div>
      </div>

      <!-- 总大小 + 倒计时 -->
      <div class="flex items-center justify-between text-[calc(12px*var(--ui-scale))]">
        <span class="text-[var(--text-secondary)]">
          {{ t('peers.transfers.totalSizeLabel') }}：
          <span class="font-medium text-[var(--text-primary)] wb-mono">{{ totalSizeLabel }}</span>
        </span>
        <span class="text-[var(--text-tertiary)]">
          {{ t('peers.transfers.countdown', { seconds: remainingSecs }) }}
        </span>
      </div>
    </div>

    <template #footer>
      <div class="flex justify-end gap-3">
        <button class="wb-btn-ghost" @click="handleReject">
          {{ t('peers.transfers.rejectAll') }}
        </button>
        <button class="wb-btn-primary" @click="handleAccept">
          {{ t('peers.transfers.acceptAll') }}
        </button>
      </div>
    </template>
  </Modal>
</template>

<script setup lang="ts">
/**
 * 传输询问弹窗宿主 — 桌面端全局挂载（issue 10）
 *
 * 展示最早到达的待应答传输批（发送方身份 + 文件清单 + 总大小 + 倒计时），
 * 接受/拒绝/关闭均经 usePeerReceiving 回执；超时自动拒绝由 composable 心跳
 * 与宿主引擎 TTL 双重兜底。其余排队批次随本批结算后依次浮现。
 */
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import Modal from '@/components/Modal.vue'
import { usePeerReceiving } from '@/composables/usePeerReceiving'

const { t } = useI18n()
const receiving = usePeerReceiving()

const offer = receiving.currentOffer

/** 短指纹展示（前 8 位，与设备列表口径一致） */
const shortFingerprint = computed(() => (offer.value?.nodeId ?? '').slice(0, 8))

const totalSizeLabel = computed(() => formatBytes(offer.value?.totalBytes ?? 0))

/** 关闭弹窗等同拒绝（与首连确认同语义；超时路径由心跳兜底） */
function handleAccept(): void {
  const batchId = offer.value?.batchId
  if (!batchId) return
  void receiving.respond(batchId, true)
}

function handleReject(): void {
  const batchId = offer.value?.batchId
  if (!batchId) return
  void receiving.respond(batchId, false)
}

/** 字节数人性化展示（KB/MB/GB；与任务页共用口径） */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = -1
  do {
    value /= 1024
    unit++
  } while (value >= 1024 && unit < units.length - 1)
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

onMounted(() => {
  void receiving.start()
})
</script>
