<template>
  <Teleport to="body">
    <Transition name="pb-dialog">
      <div
        v-if="offer"
        class="fixed inset-0 z-50 flex items-center justify-center px-6 bg-[var(--mobile-overlay)]"
        @click.self="handleReject"
      >
        <div
          class="w-full max-w-sm rounded-2xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)] shadow-xl overflow-hidden"
        >
          <!-- Header -->
          <div class="px-5 pt-5 pb-3">
            <h2 class="text-base font-semibold text-[var(--mobile-text-primary)]">
              {{ t('peers.transfers.askTitle') }}
            </h2>
            <p class="mt-1 text-[13px] text-[var(--mobile-text-secondary)] truncate">
              {{ t('peers.transfers.fromWithName', { name: offer.peerName }) }}
              <span class="ml-1 font-mono text-xs text-[var(--mobile-text-tertiary)]">
                {{ shortFingerprint }}
              </span>
            </p>
          </div>

          <!-- 文件清单（滚动区） -->
          <div class="px-5">
            <p class="text-xs text-[var(--mobile-text-tertiary)] mb-1.5 flex items-center justify-between">
              <span>{{ t('peers.transfers.fileList') }}</span>
              <span>{{ t('peers.transfers.filesCount', { count: offer.files.length }) }}</span>
            </p>
            <div
              class="rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)] max-h-44 overflow-y-auto overflow-x-hidden"
            >
              <div
                v-for="(file, index) in offer.files"
                :key="index"
                class="px-3 py-2 flex items-center justify-between gap-3"
                :class="index > 0 ? 'border-t border-[var(--mobile-border)]' : ''"
              >
                <span class="text-[13px] text-[var(--mobile-text-secondary)] truncate min-w-0">
                  {{ file.path }}
                </span>
                <span class="font-mono text-xs text-[var(--mobile-text-tertiary)] flex-shrink-0">
                  {{ formatBytes(file.size) }}
                </span>
              </div>
            </div>
            <div class="flex items-center justify-between py-3 text-[13px]">
              <span class="text-[var(--mobile-text-secondary)]">
                {{ t('peers.transfers.totalSizeLabel') }}
                <span class="font-medium text-[var(--mobile-text-primary)] font-mono ml-0.5">
                  {{ totalSizeLabel }}
                </span>
              </span>
              <span class="text-[var(--mobile-text-tertiary)]">
                {{ t('peers.transfers.countdown', { seconds: remainingSecs }) }}
              </span>
            </div>
          </div>

          <!-- Actions（44px 触达） -->
          <div class="flex border-t border-[var(--mobile-border)]">
            <button
              class="flex-1 h-11 text-sm font-medium text-[var(--mobile-text-secondary)] active:opacity-70 transition-opacity duration-150"
              @click="handleReject"
            >
              {{ t('peers.transfers.rejectAll') }}
            </button>
            <div class="w-px bg-[var(--mobile-border)]"></div>
            <button
              class="flex-1 h-11 text-sm font-semibold text-[var(--mobile-accent)] active:opacity-70 transition-opacity duration-150"
              @click="handleAccept"
            >
              {{ t('peers.transfers.acceptAll') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 传输询问弹窗宿主 — 移动端全局挂载（issue 10）
 *
 * 展示最早到达的待应答传输批（发送方身份 + 文件清单 + 总大小 + 倒计时），
 * 接受/拒绝/关闭均经 usePeerReceiving 回执；超时自动拒绝由 composable 心跳
 * 与宿主引擎 TTL 双重兜底。其余排队批次随本批结算后依次浮现。
 */
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
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

<style scoped>
/* 弹窗出入场：淡入 + 轻微缩放（GPU 合成属性；<300ms） */
.pb-dialog-enter-active,
.pb-dialog-leave-active {
  transition: opacity 0.2s ease;
}
.pb-dialog-enter-active > div,
.pb-dialog-leave-active > div {
  transition: transform 0.2s ease;
}
.pb-dialog-enter-from,
.pb-dialog-leave-to {
  opacity: 0;
}
.pb-dialog-enter-from > div {
  transform: scale(0.96);
}
.pb-dialog-leave-to > div {
  transform: scale(0.98);
}
</style>
