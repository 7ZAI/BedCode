<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-page px-8 h-14 flex items-center">
      <h2 class="text-[var(--font-size-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.title') }}</h2>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <!-- QR Code Section -->
      <div class="bg-card rounded-card p-6 shadow-card mb-6 animate-fade-slide-up">
        <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.qrTitle') }}</h3>
        <p class="text-[var(--text-secondary)] text-[13px] mt-1 mb-5">{{ t('desktop.device.qrDesc') }}</p>

        <div v-if="!qr.hasQr.value" class="text-center py-4">
          <Button variant="secondary" @click="qr.generateQr(selectedIp || undefined)" :loading="qr.isLoading.value">
            <template #icon>
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
              </svg>
            </template>
            {{ t('desktop.device.generateQr') }}
          </Button>
        </div>

        <div v-else class="text-center py-4">
          <p class="text-[var(--text-secondary)] mb-4">{{ t('desktop.device.qrHint') }}</p>

          <!-- QR Code Canvas -->
          <div class="inline-block bg-white p-4 rounded-lg mb-4">
            <canvas ref="qrCanvasRef" class="w-48 h-48"></canvas>
          </div>

          <p class="text-[var(--text-secondary)] text-sm mb-2">
            {{ t('desktop.device.qrValidity') }}
            <span class="text-brand font-medium">{{ qr.remainingSeconds.value }}</span> {{ t('common.time.seconds') }}
          </p>

          <p class="text-[var(--text-tertiary)] text-xs">
            {{ t('desktop.device.qrSingleUse') }}
          </p>

          <div class="flex items-center justify-center gap-3 mt-4">
            <Button variant="ghost" size="sm" @click="qr.clearQr()">
              {{ t('common.button.cancel') }}
            </Button>
            <Button variant="ghost" size="sm" @click="qr.generateQr(selectedIp || undefined)" :loading="qr.isLoading.value">
              {{ t('common.button.refresh') }}
            </Button>
          </div>
        </div>
      </div>

      <!-- Pairing Section -->
      <div class="bg-card rounded-card p-6 shadow-card mb-6 animate-fade-slide-up" style="animation-delay: 50ms">
        <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.pairingCodeTitle') }}</h3>
        <p class="text-[var(--text-secondary)] text-[13px] mt-1 mb-5">{{ t('desktop.device.pairingCodeDesc') }}</p>

        <div v-if="!pairingCode" class="text-center py-4">
          <p class="text-[var(--text-secondary)] mb-4">{{ t('desktop.device.pairingCodeDesc') }}</p>
          <Button variant="primary" @click="generateCode" :loading="isLoading">
            <template #icon>
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
              </svg>
            </template>
            {{ t('desktop.device.generateCode') }}
          </Button>
        </div>

        <div v-else class="text-center py-4">
          <p class="text-[var(--text-secondary)] mb-4">{{ t('desktop.device.pairingCodeHint') }}</p>

          <!-- Pairing Code Display -->
          <div class="text-5xl font-mono font-bold text-brand tracking-widest mb-4">
            {{ pairingCode.code }}
          </div>

          <p class="text-[var(--text-secondary)] text-sm mb-6">
            {{ t('desktop.device.codeExpiresIn', { seconds: remainingSeconds }) }}
          </p>

          <Button variant="ghost" size="sm" @click="cancelPairing">
            {{ t('common.button.cancel') }}
          </Button>
        </div>
      </div>

      <!-- Network Info -->
      <div class="bg-card rounded-card p-6 shadow-card mb-6 animate-fade-slide-up" style="animation-delay: 100ms">
        <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.networkTitle') }}</h3>
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-[var(--text-secondary)]">{{ t('desktop.device.websocketPort') }}</span>
            <span class="font-mono text-[var(--text-primary)]">{{ port }}</span>
          </div>
          <div class="flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <span class="text-[var(--text-secondary)]">{{ t('desktop.device.ipv4Address') }}</span>
              <div class="flex items-center gap-2">
                <span class="font-mono text-sm bg-[var(--bg-hover)] px-2 py-1 rounded-input text-[var(--text-primary)]">
                  {{ displayIp }}
                </span>
                <button
                  @click="showIpSelector = true"
                  class="text-sm text-brand hover:text-[var(--color-primary-hover)]"
                >
                  {{ t('desktop.device.select') }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- IP Selector Modal -->
      <Modal v-model="showIpSelector" :title="t('desktop.device.selectIpTitle')">
        <div class="space-y-2">
          <p class="text-sm text-[var(--text-secondary)] mb-4">{{ t('desktop.device.selectIpDesc') }}</p>
          <div
            v-for="ip in ipv4Addresses"
            :key="ip"
            @click="selectIp(ip)"
            :class="[
              'p-3 rounded-input cursor-pointer border transition-colors',
              selectedIp === ip
                ? 'border-brand bg-brand-light'
                : 'border-[var(--border)] hover:border-brand'
            ]"
          >
            <span class="font-mono">{{ ip }}</span>
          </div>
          <p v-if="ipv4Addresses.length === 0" class="text-[var(--text-tertiary)] text-center py-4">
            {{ t('desktop.device.noIpv4') }}
          </p>
        </div>
        <div class="mt-4 flex justify-end">
          <Button variant="ghost" @click="showIpSelector = false">{{ t('common.button.cancel') }}</Button>
        </div>
      </Modal>

      <!-- Paired Devices -->
      <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up" style="animation-delay: 150ms">
        <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.pairedTitle') }}</h3>

        <div v-if="deviceStore.pairedDevices.length === 0" class="text-center py-8">
          <svg class="w-12 h-12 mx-auto text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
          <p class="text-[var(--text-secondary)]">{{ t('desktop.device.noPaired') }}</p>
        </div>

        <div v-else class="space-y-3">
          <div
            v-for="device in deviceStore.pairedDevices"
            :key="device.id"
            class="flex items-center justify-between p-4 bg-[var(--bg-hover)]/50 rounded-input"
          >
            <div class="flex items-center gap-4">
              <!-- Status Indicator (live WebSocket status) -->
              <div
                :class="[
                  'w-3 h-3 rounded-full shrink-0',
                  isDeviceOnline(device) ? 'bg-green-500 animate-pulse' : 'bg-dark-500'
                ]"
              ></div>

              <div>
                <p class="font-medium">{{ device.deviceName }}</p>
                <div class="flex items-center gap-3 text-[var(--text-tertiary)] text-xs mt-1">
                  <span>{{ t('desktop.device.pairedAt', { date: formatDate(device.pairedAt) }) }}</span>
                  <span v-if="device.lastSeen" class="text-[var(--text-tertiary)]">|</span>
                  <span v-if="device.lastSeen">{{ t('desktop.device.lastSeen', { date: formatDate(device.lastSeen) }) }}</span>
                  <span class="text-[var(--text-tertiary)]">|</span>
                  <span>{{ t('desktop.device.connectCount', { count: device.connectCount }) }}</span>
                </div>
              </div>
            </div>

            <div class="flex items-center gap-3">
              <span
                :class="[
                  'text-xs px-2.5 py-1 rounded-tag font-medium',
                  isDeviceOnline(device) ? 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400' : 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
                ]"
              >
                {{ isDeviceOnline(device) ? t('desktop.device.connected') : t('desktop.device.offline') }}
              </span>

              <Button variant="ghost" size="sm" @click="removeDevice(device.id)">
                <svg class="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Remove Device Confirm Dialog -->
    <Modal v-model="showRemoveDeviceDialog" :title="t('desktop.device.confirmRemove')" size="sm">
      <p class="text-[var(--text-primary)]">{{ t('desktop.device.confirmRemoveMsg') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showRemoveDeviceDialog = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" @click="confirmRemoveDevice">{{ t('common.button.remove') }}</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/device'
import { useSettingsStore } from '@/stores/settings'
import { usePairing, useNetwork, useConnectedDevices, type DeviceConnectionInfo, type PairingCodeInfo } from '@/composables/useTauri'
import type { PairedDevice } from '@/stores/device'
import { useQrCode } from '@/composables/useQrCode'
import { listen } from '@tauri-apps/api/event'
import Button from '@/components/Button.vue'
import Modal from '@/components/Modal.vue'
import { useToast } from '@/composables/useToast'
import QRCode from 'qrcode'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const pairing = usePairing()
const network = useNetwork()
const connected = useConnectedDevices()
const toast = useToast()

// 从配置获取端口
const port = computed(() => settingsStore.settings.network.port)
const qrHost = computed(() => settingsStore.settings.network.qr_host)

// 显示的 IP（优先使用配置的 qr_host，否则显示 "未选择"）
const displayIp = computed(() => qrHost.value || t('desktop.device.notSelected'))

// 选中的 IP 用于 QR 码生成（从配置初始化）
const selectedIp = ref<string | null>(qrHost.value || null)

// 选择 IP 地址并保存到配置
async function selectIp(ip: string) {
  selectedIp.value = ip
  showIpSelector.value = false
  // 保存到配置
  await settingsStore.saveSettings({
    network: { ...settingsStore.settings.network, qr_host: ip }
  })
}

// Real-time connected device fingerprints (from WebSocket events)
const connectedFingerprints = ref<Set<string>>(new Set())

// 检查设备是否实时在线（通过 fingerprint 匹配数据库 pairings 记录）
function isDeviceOnline(device: PairedDevice): boolean {
  return connectedFingerprints.value.has(device.deviceFingerprint)
}

const isLoading = ref(false)
const showIpSelector = ref(false)
const pairingCode = ref<PairingCodeInfo | null>(null)
const remainingSeconds = ref(0)

const localAddresses = computed(() => network.localAddresses.value)

// 分类 IP 地址
const ipv4Addresses = computed(() => {
  return localAddresses.value.filter(ip => ip.includes('.'))
})

let countdownInterval: ReturnType<typeof setInterval> | null = null
let pairingCodeListener: (() => void) | null = null

const qr = useQrCode()
const qrCanvasRef = ref<HTMLCanvasElement | null>(null)
const showRemoveDeviceDialog = ref(false)
const pendingDeviceId = ref<string | null>(null)

// 当 QR 数据变化时渲染 Canvas
watch(
  () => qr.qrData.value,
  async (data) => {
    if (data && qrCanvasRef.value) {
      const qrContent = JSON.stringify({
        host: data.host,
        port: data.port,
        token: data.token,
      })
      await QRCode.toCanvas(qrCanvasRef.value, qrContent, {
        width: 192,
        margin: 2,
        color: {
          dark: '#000000',
          light: '#ffffff',
        },
      })
    }
  },
  { flush: 'post' }
)

let deviceConnectedListener: (() => void) | null = null
let deviceDisconnectedListener: (() => void) | null = null
let qrTokenConsumedListener: (() => void) | null = null

onMounted(async () => {
  await settingsStore.loadSettings()
  await deviceStore.loadPairedDevices()
  await network.loadLocalAddresses()

  // 如果配置中没有 qr_host，自动选择一个合适的 IP
  if (!settingsStore.settings.network.qr_host && ipv4Addresses.value.length > 0) {
    selectedIp.value = ipv4Addresses.value[0]
    await settingsStore.saveSettings({
      network: { ...settingsStore.settings.network, qr_host: selectedIp.value }
    })
  } else {
    selectedIp.value = qrHost.value || null
  }

  // Load initial connected device list
  await connected.loadConnectedDevices()
  const fingerprints = new Set<string>(
    connected.connectedDevices.value
      .map((d: any) => d.fingerprint)
      .filter((fp: string | undefined): fp is string => !!fp)
  )
  connectedFingerprints.value = fingerprints

  // 尝试恢复现有二维码（不重新生成）
  const qrRestored = await qr.restoreQr(selectedIp.value || undefined)
  if (qrRestored) {
    console.log('Restored active QR token')
  }

  // 检查是否有活跃的配对码，若有则自动恢复显示
  const hasActiveCode = await pairing.checkCurrentCode()
  if (hasActiveCode && pairing.pairingCode.value) {
    console.log('Restoring active pairing code:', pairing.pairingCode.value)
    pairingCode.value = pairing.pairingCode.value
    // 使用后端返回的剩余时间（expires_in 已是实际剩余时间）
    remainingSeconds.value = pairing.pairingCode.value.expires_in
    startCountdown()
  }

  // Listen for real-time device connection events
  deviceConnectedListener = await listen<DeviceConnectionInfo>('device-connected', async (event) => {
    // 通过 fingerprint 追踪在线设备，而非 device_id
    const fp = (event.payload as any).fingerprint
    if (fp) {
      connectedFingerprints.value = new Set([...connectedFingerprints.value, fp])
    }

    // 刷新配对设备列表（认证成功后后端已写入数据库）
    await deviceStore.loadPairedDevices()

    // 当有设备连接成功后，清除已使用的配对码并刷新显示
    if (pairingCode.value) {
      console.log('Device connected, clearing pairing code...')
      pairing.clearCode()
      pairingCode.value = null
      remainingSeconds.value = 0
      if (countdownInterval) {
        clearInterval(countdownInterval)
        countdownInterval = null
      }
    }
  })

  // 监听 QR token 被消耗事件，自动重新生成二维码
  qrTokenConsumedListener = await listen('qr-token-consumed', () => {
    console.log('QR token consumed, regenerating QR code...')
    qr.generateQr(selectedIp.value || undefined)
    toast.success(t('desktop.device.deviceConnected'))
  })
  deviceDisconnectedListener = await listen<DeviceConnectionInfo>('device-disconnected', (event) => {
    const fp = (event.payload as any).fingerprint
    if (fp) {
      const newSet = new Set(connectedFingerprints.value)
      newSet.delete(fp)
      connectedFingerprints.value = newSet
    }
  })

  // 监听配对码自动生成事件
  pairingCodeListener = await listen<{ code: string; expires_in: number; device_name?: string }>(
    'pairing-code-generated',
    (event) => {
      console.log('Received pairing-code-generated event:', event.payload)
      pairingCode.value = {
        code: event.payload.code,
        expires_in: event.payload.expires_in,
        created_at: new Date().toISOString(),
      }
      remainingSeconds.value = event.payload.expires_in
      startCountdown()

      toast.info(t('desktop.device.pairingRequest', { code: event.payload.code }))
    }
  )
})

onUnmounted(() => {
  if (countdownInterval) {
    clearInterval(countdownInterval)
  }
  if (pairingCodeListener) {
    pairingCodeListener()
  }
  if (deviceConnectedListener) {
    deviceConnectedListener()
  }
  if (deviceDisconnectedListener) {
    deviceDisconnectedListener()
  }
  if (qrTokenConsumedListener) {
    qrTokenConsumedListener()
  }
  // 不清除 QR 码和配对码，保持状态以便下次进入页面时恢复
})

// 启动配对码倒计时
function startCountdown() {
  if (countdownInterval) {
    clearInterval(countdownInterval)
    countdownInterval = null
  }
  countdownInterval = setInterval(() => {
    if (remainingSeconds.value > 0) {
      remainingSeconds.value--
    } else {
      // 配对码过期，清除后端状态
      pairing.clearCode()
      pairingCode.value = null
      if (countdownInterval) {
        clearInterval(countdownInterval)
        countdownInterval = null
      }
    }
  }, 1000)
}

async function generateCode() {
  // 清除之前的倒计时
  if (countdownInterval) {
    clearInterval(countdownInterval)
    countdownInterval = null
  }

  isLoading.value = true
  try {
    await pairing.generateCode()
    pairingCode.value = pairing.pairingCode.value

    if (pairingCode.value && pairingCode.value.code) {
      remainingSeconds.value = pairingCode.value.expires_in
      startCountdown()
    } else {
      toast.error(t('desktop.device.codeGenerateFailedNoCode'))
    }
  } catch (e) {
    console.error('生成配对码失败:', e)
    toast.error(t('desktop.device.codeGenerateFailed'))
  } finally {
    isLoading.value = false
  }
}

function cancelPairing() {
  // 通知后端清除配对码
  pairing.clearCode()
  pairingCode.value = null
  remainingSeconds.value = 0
  if (countdownInterval) {
    clearInterval(countdownInterval)
    countdownInterval = null
  }
}

async function removeDevice(deviceId: string) {
  pendingDeviceId.value = deviceId
  showRemoveDeviceDialog.value = true
}

async function confirmRemoveDevice() {
  if (!pendingDeviceId.value) return
  await deviceStore.removeDevice(pendingDeviceId.value)
  toast.success(t('desktop.device.deviceRemoved'))
  showRemoveDeviceDialog.value = false
  pendingDeviceId.value = null
}

function formatDate(dateStr: string): string {
  if (!dateStr || dateStr === '') {
    return t('common.status.unknown')
  }
  const date = new Date(dateStr)
  if (isNaN(date.getTime())) {
    return t('common.status.unknown')
  }
  return date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
</script>
