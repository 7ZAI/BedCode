<template>
  <Modal
    :model-value="!!request"
    :title="t('settings.peer.consentTitle')"
    size="sm"
    @close="consent.deny()"
  >
    <div class="flex flex-col items-center gap-4 py-2">
      <!-- 设备图标 -->
      <div
        class="w-14 h-14 rounded-full bg-[var(--color-primary-light)] flex items-center justify-center flex-shrink-0"
      >
        <svg
          class="w-7 h-7 text-[var(--color-primary)]"
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

      <!-- 描述：有设备名展示名，无名以短指纹指代 -->
      <p
        class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)] text-center leading-relaxed max-w-xs break-words"
      >
        <template v-if="request?.deviceName">
          {{ t('settings.peer.consentWithName', { name: request.deviceName }) }}
        </template>
        <template v-else>{{ t('settings.peer.consentWithoutName') }}</template>
      </p>

      <!-- 完整节点 ID（指纹核对依据） -->
      <div
        class="w-full rounded-[8px] border border-[var(--border)] bg-[var(--bg-page)] px-3 py-2.5 flex items-center justify-between gap-3 min-w-0"
      >
        <span class="text-xs text-[var(--text-tertiary)] flex-shrink-0">
          {{ t('settings.peer.fingerprintLabel') }}
        </span>
        <code class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] truncate">
          {{ request?.nodeId }}
        </code>
      </div>

      <p class="text-xs text-[var(--text-tertiary)]">
        {{ t('settings.peer.consentCountdown', { seconds: remainingSeconds }) }}
      </p>
    </div>

    <template #footer>
      <div class="flex justify-end gap-3">
        <button class="wb-btn-ghost" @click="consent.deny()">
          {{ t('settings.peer.consentDeny') }}
        </button>
        <button class="wb-btn-primary" @click="consent.accept()">
          {{ t('settings.peer.consentAccept') }}
        </button>
      </div>
    </template>
  </Modal>
</template>

<script setup lang="ts">
/**
 * 首连确认弹窗宿主 — 桌面端全局挂载（issue 04）
 *
 * 展示当前待确认请求（设备名 + 完整指纹 + 倒计时），接受/拒绝/关闭均经
 * usePeerConsent 结算；终端配对迁移规则的自动互信在 composable 内完成、
 * 不走本弹窗，仅经 watch 弹 toast 告知用户。
 */
import { onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import Modal from '@/components/Modal.vue'
import { usePeerConsent, PEER_CONSENT_TIMEOUT_MS } from '@/composables/usePeerConsent'
import { useDeviceStore } from '@/stores/device'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const consent = usePeerConsent()
const deviceStore = useDeviceStore()
const toast = useToast()

const request = consent.currentRequest

// 倒计时展示：随请求重置，每秒递减到 0（实际结算由 composable 定时器执行）
const remainingSeconds = ref(PEER_CONSENT_TIMEOUT_MS / 1000)
let countdownInterval: ReturnType<typeof setInterval> | null = null

function stopCountdown() {
  if (countdownInterval) {
    clearInterval(countdownInterval)
    countdownInterval = null
  }
}

watch(request, (value) => {
  stopCountdown()
  if (!value) return
  remainingSeconds.value = PEER_CONSENT_TIMEOUT_MS / 1000
  countdownInterval = setInterval(() => {
    if (remainingSeconds.value > 0) remainingSeconds.value--
  }, 1000)
})

onBeforeUnmount(stopCountdown)

// 迁移规则自动互信提示：非弹窗路径的用户告知（composable 只记名字，不碰 UI）
watch(consent.autoTrustedName, (name) => {
  if (name) {
    toast.info(t('settings.peer.autoTrustedToast', { name }))
    consent.autoTrustedName.value = null
  }
})

// 配对名单实时拉取：判定时才读 store，反映最新配对状态（含未进过设备页的场景）
void consent.start(async () => {
  await deviceStore.loadPairedDevices()
  return deviceStore.pairedDevices.map((device) => device.deviceName)
})
</script>
