<template>
  <ConfirmDialog
    :model-value="!!request"
    :title="$t('settings.peer.consentTitle')"
    :message="message"
    variant="info"
    :confirm-text="$t('settings.peer.consentAccept')"
    :cancel-text="$t('settings.peer.consentDeny')"
    :close-on-backdrop="false"
    @confirm="consent.accept()"
    @cancel="consent.deny()"
  />
</template>

<script setup lang="ts">
/**
 * 首连确认弹窗宿主 — 移动端全局挂载（issue 04）
 *
 * 展示当前待确认请求（设备名 + 短指纹 + 倒计时），接受/拒绝均经
 * usePeerConsent 结算；终端配对迁移规则的自动互信在 composable 内完成、
 * 不走本弹窗，仅经 watch 弹 toast 告知用户。完整节点 ID 过长不适合移动端
 * 弹窗展示，核对以短指纹为准（与发现列表一致）。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import { usePeerConsent, PEER_CONSENT_TIMEOUT_MS } from '@/composables/usePeerConsent'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const consent = usePeerConsent()
const connection = useMobileConnection()
const toast = useToast()

const request = consent.currentRequest

// 倒计时提示文案：随请求重置，每秒递减（实际结算由 composable 定时器执行）
const remainingSeconds = ref(PEER_CONSENT_TIMEOUT_MS / 1000)

const message = computed(() => {
  const namePart = request.value?.deviceName
    ? t('settings.peer.consentWithName', { name: request.value.deviceName })
    : t('settings.peer.consentWithoutName')
  const fingerprintPart = t('settings.peer.fingerprintShortLabel', {
    fingerprint: request.value?.fingerprintShort ?? '',
  })
  return `${namePart}\n${fingerprintPart}\n${t('settings.peer.consentCountdown', { seconds: Math.max(remainingSeconds.value, 0) })}`
})

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
    remainingSeconds.value--
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

// 配对名单实时拉取：判定时才读 localStorage，反映最新配对状态
void consent.start(async () => {
  connection.loadPairedDevices()
  return connection.pairedDevices.value.map((device) => device.name)
})
</script>
