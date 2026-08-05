<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+IP:端口，右刷新/生成 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3">
        <h2 class="text-[13px] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.title') }}</h2>
        <span class="wb-mono text-[12px] text-[var(--text-tertiary)]">{{ displayIp }}:{{ port }}</span>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="devices" />
        <button class="wb-btn-ghost" @click="refreshDevices">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99" />
          </svg>
          {{ t('common.button.refresh') }}
        </button>
        <button class="wb-btn-primary" :disabled="isLoading" @click="generateCode">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
          {{ t('desktop.device.generateCode') }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-6 space-y-6">
      <!-- ==================== 配对区：二维码 + 配对码 + IP 选择 ==================== -->
      <section>
        <h3 class="wb-section-title">PAIRING · {{ displayIp }}:{{ port }}</h3>
        <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-5 grid gap-6 md:grid-cols-2">
          <!-- 二维码 -->
          <div>
            <h4 class="text-[13px] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.qrTitle') }}</h4>
            <p class="text-[12px] text-[var(--text-secondary)] mt-1">{{ t('desktop.device.qrDesc') }}</p>
            <div class="mt-4 flex items-start gap-4">
              <!-- 白底衬底保证二维码在暗色模式下可读 -->
              <div class="shrink-0 inline-block bg-white p-2 rounded-lg border border-[var(--border)]">
                <canvas ref="qrCanvasRef" class="block"></canvas>
              </div>
              <div class="min-w-0 text-[12px] space-y-1.5 pt-1">
                <template v-if="qr.hasQr.value">
                  <p class="text-[var(--text-secondary)]">{{ t('desktop.device.qrHint') }}</p>
                  <p class="text-[var(--text-secondary)]">
                    {{ t('desktop.device.qrValidity') }}
                    <span class="wb-mono font-medium text-[var(--text-primary)]">{{ qr.remainingSeconds.value }}</span>
                    {{ t('common.time.seconds') }}
                  </p>
                  <p class="text-[11px] text-[var(--text-tertiary)]">{{ t('desktop.device.qrSingleUse') }}</p>
                  <div class="flex items-center gap-2 pt-1">
                    <button class="wb-btn-ghost !h-6 !px-2 text-[11px]" @click="qr.clearQr()">
                      {{ t('common.button.cancel') }}
                    </button>
                    <button class="wb-btn-ghost !h-6 !px-2 text-[11px]" :disabled="qr.isLoading.value" @click="qr.generateQr(selectedIp || undefined)">
                      {{ t('common.button.refresh') }}
                    </button>
                  </div>
                </template>
                <template v-else>
                  <p class="text-[var(--text-secondary)]">{{ t('desktop.device.qrHint') }}</p>
                  <button class="wb-btn-ghost !h-6 !px-2 text-[11px]" :disabled="qr.isLoading.value" @click="qr.generateQr(selectedIp || undefined)">
                    {{ t('desktop.device.generateQr') }}
                  </button>
                </template>
              </div>
            </div>
          </div>

          <!-- 配对码 -->
          <div class="md:border-l md:border-[var(--border)] md:pl-6">
            <h4 class="text-[13px] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.pairingCodeTitle') }}</h4>
            <p class="text-[12px] text-[var(--text-secondary)] mt-1">{{ t('desktop.device.pairingCodeDesc') }}</p>
            <div v-if="!pairingCode" class="mt-4">
              <p class="text-[12px] text-[var(--text-secondary)] mb-2">{{ t('desktop.device.pairingCodeHint') }}</p>
              <button class="wb-btn-primary" :disabled="isLoading" @click="generateCode">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
                </svg>
                {{ t('desktop.device.generateCode') }}
              </button>
            </div>
            <div v-else class="mt-4">
              <p class="text-[12px] text-[var(--text-secondary)] mb-2">{{ t('desktop.device.pairingCodeHint') }}</p>
              <p class="text-4xl font-mono font-bold tracking-[0.2em] text-[var(--text-primary)]">{{ pairingCode.code }}</p>
              <p class="text-[12px] text-[var(--text-secondary)] mt-3">
                {{ t('desktop.device.codeExpiresIn', { seconds: remainingSeconds }) }}
              </p>
              <button class="wb-btn-ghost !h-6 !px-2 text-[11px] mt-3" @click="cancelPairing">
                {{ t('common.button.cancel') }}
              </button>
            </div>
          </div>

          <!-- IP 选择 + 端口 -->
          <div class="md:col-span-2 border-t border-[var(--border)] pt-4 flex items-center gap-3 flex-wrap">
            <span class="text-[12px] text-[var(--text-secondary)]">{{ t('desktop.device.ipv4Address') }}</span>
            <select
              :value="selectedIp || ''"
              class="h-7 px-2 rounded-[6px] border border-[var(--border)] bg-[var(--bg-card)] wb-mono text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
              @change="onIpSelect(($event.target as HTMLSelectElement).value)"
            >
              <option v-if="!selectedIp" value="" disabled>{{ t('desktop.device.notSelected') }}</option>
              <option v-for="ip in ipv4Addresses" :key="ip" :value="ip">{{ ip }}</option>
            </select>
            <span class="wb-mono text-[12px] text-[var(--text-tertiary)]">:{{ port }}</span>
            <span v-if="ipv4Addresses.length === 0" class="text-[12px] text-[var(--text-tertiary)]">{{ t('desktop.device.noIpv4') }}</span>
          </div>
        </div>
      </section>

      <!-- ==================== ONLINE 分区 ==================== -->
      <section>
        <h3 class="wb-section-title">ONLINE · {{ onlineDevices.length }}</h3>
        <p v-if="onlineDevices.length === 0" class="wb-mono text-[12px] text-[var(--text-tertiary)] px-1 py-2">
          {{ t('common.misc.noData') }}
        </p>
        <div v-else class="space-y-2">
          <div
            v-for="device in onlineDevices"
            :key="device.id"
            class="flex items-center justify-between gap-4 px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] hover:shadow-sm transition-shadow"
          >
            <div class="flex items-center gap-3 min-w-0">
              <span class="w-2 h-2 rounded-full shrink-0 bg-[var(--color-success)]"></span>
              <div class="min-w-0">
                <p class="text-[13px] font-medium text-[var(--text-primary)] truncate">{{ device.deviceName }}</p>
                <p class="text-[11px] text-[var(--text-tertiary)] truncate mt-0.5">
                  {{ t('desktop.device.pairedAt', { date: formatDate(device.pairedAt) }) }}
                  <template v-if="device.lastSeen"> · {{ t('desktop.device.lastSeen', { date: formatDate(device.lastSeen) }) }}</template>
                  · {{ t('desktop.device.connectCount', { count: device.connectCount }) }}
                </p>
              </div>
            </div>
            <div class="flex items-center gap-3 shrink-0">
              <div class="text-right">
                <p class="wb-mono text-[12.5px] text-[var(--text-primary)]">{{ device.address }}</p>
                <p class="wb-mono text-[11px] mt-0.5 text-green-600 dark:text-green-400">{{ t('desktop.device.connected') }}</p>
              </div>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-[var(--border)] wb-mono text-[11px] uppercase tracking-wide text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                @click="viewHistory(device.id)"
              >
                {{ t('desktop.device.historyView') }}
              </button>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[11px] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
                @click="removeDevice(device.id)"
              >
                {{ t('common.button.remove') }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- ==================== OFFLINE 分区 ==================== -->
      <section>
        <h3 class="wb-section-title">OFFLINE · {{ offlineDevices.length }}</h3>
        <p v-if="offlineDevices.length === 0" class="wb-mono text-[12px] text-[var(--text-tertiary)] px-1 py-2">
          {{ t('common.misc.noData') }}
        </p>
        <div v-else class="space-y-2">
          <div
            v-for="device in offlineDevices"
            :key="device.id"
            class="flex items-center justify-between gap-4 px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] hover:shadow-sm transition-shadow"
          >
            <div class="flex items-center gap-3 min-w-0">
              <span class="w-2 h-2 rounded-full shrink-0 bg-[var(--text-tertiary)]"></span>
              <div class="min-w-0">
                <p class="text-[13px] font-medium text-[var(--text-secondary)] truncate">{{ device.deviceName }}</p>
                <p class="text-[11px] text-[var(--text-tertiary)] truncate mt-0.5">
                  {{ t('desktop.device.pairedAt', { date: formatDate(device.pairedAt) }) }}
                  <template v-if="device.lastSeen"> · {{ t('desktop.device.lastSeen', { date: formatDate(device.lastSeen) }) }}</template>
                  · {{ t('desktop.device.connectCount', { count: device.connectCount }) }}
                </p>
              </div>
            </div>
            <div class="flex items-center gap-3 shrink-0">
              <div class="text-right">
                <p class="wb-mono text-[12.5px] text-[var(--text-primary)]">{{ device.address }}</p>
                <p class="wb-mono text-[11px] mt-0.5 text-[var(--text-tertiary)]">{{ t('desktop.device.offline') }}</p>
              </div>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-[var(--border)] wb-mono text-[11px] uppercase tracking-wide text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                @click="viewHistory(device.id)"
              >
                {{ t('desktop.device.historyView') }}
              </button>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[11px] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
                @click="removeDevice(device.id)"
              >
                {{ t('common.button.remove') }}
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>

    <!-- 移除设备确认 -->
    <Modal v-model="showRemoveDeviceDialog" :title="t('desktop.device.confirmRemove')" size="sm">
      <p class="text-[var(--text-primary)] text-[13px]">{{ t('desktop.device.confirmRemoveMsg') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showRemoveDeviceDialog = false">{{ t('common.button.cancel') }}</button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" @click="confirmRemoveDevice">{{ t('common.button.remove') }}</button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * 设备视图 — 桌面端设备配对与设备列表
 * Warm Workbench 风格：PAIRING 配对区 + ONLINE/OFFLINE 分区；QR/配对码/实时在线全为真实逻辑
 */
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/device'
import { useSettingsStore } from '@/stores/settings'
import { usePairing, useNetwork, useConnectedDevices, type DeviceConnectionInfo, type PairingCodeInfo } from '@/composables/useTauri'
import type { PairedDevice } from '@/stores/device'
import { useQrCode } from '@/composables/useQrCode'
import { listen } from '@tauri-apps/api/event'
import Modal from '@/components/Modal.vue'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import { useToast } from '@/composables/useToast'
import QRCode from 'qrcode'

const { t } = useI18n()
const router = useRouter()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const pairing = usePairing()
const network = useNetwork()
const connected = useConnectedDevices()
const toast = useToast()

// 从配置获取端口与 QR host
const port = computed(() => settingsStore.settings.network.port)
const qrHost = computed(() => settingsStore.settings.network.qr_host)

// 显示的 IP（优先使用配置的 qr_host，否则显示"未选择"）
const displayIp = computed(() => qrHost.value || t('desktop.device.notSelected'))

// 选中的 IP 用于 QR 码生成（从配置初始化）
const selectedIp = ref<string | null>(qrHost.value || null)

// 实时在线设备指纹（来自 WebSocket 事件）
const connectedFingerprints = ref<Set<string>>(new Set())

const isLoading = ref(false)
const pairingCode = ref<PairingCodeInfo | null>(null)
const remainingSeconds = ref(0)
const showRemoveDeviceDialog = ref(false)
const pendingDeviceId = ref<string | null>(null)

const localAddresses = computed(() => network.localAddresses.value)

// 分类 IPv4 地址
const ipv4Addresses = computed(() => {
  return localAddresses.value.filter(ip => ip.includes('.'))
})

/** 设备是否实时在线（通过 fingerprint 匹配） */
function isDeviceOnline(device: PairedDevice): boolean {
  return connectedFingerprints.value.has(device.deviceFingerprint)
}

const onlineDevices = computed(() => deviceStore.pairedDevices.filter(d => isDeviceOnline(d)))
const offlineDevices = computed(() => deviceStore.pairedDevices.filter(d => !isDeviceOnline(d)))

const qr = useQrCode()
const qrCanvasRef = ref<HTMLCanvasElement | null>(null)

let countdownInterval: ReturnType<typeof setInterval> | null = null
let pairingCodeListener: (() => void) | null = null
let deviceConnectedListener: (() => void) | null = null
let deviceDisconnectedListener: (() => void) | null = null
let qrTokenConsumedListener: (() => void) | null = null

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
        width: 168,
        margin: 2,
        color: {
          dark: '#000000',
          light: '#ffffff',
        },
      })
    }
  },
  { flush: 'post' },
)

/** 选择 IP 并保存到配置（同步更新 QR 载荷） */
async function onIpSelect(ip: string) {
  selectedIp.value = ip
  await settingsStore.saveSettings({
    network: { ...settingsStore.settings.network, qr_host: ip },
  })
}

/** 刷新设备列表与在线状态 */
async function refreshDevices() {
  await deviceStore.loadPairedDevices()
  await connected.loadConnectedDevices()
  const fingerprints = new Set<string>(
    connected.connectedDevices.value
      .map((d: any) => d.fingerprint)
      .filter((fp: string | undefined): fp is string => !!fp),
  )
  connectedFingerprints.value = fingerprints
}

onMounted(async () => {
  await settingsStore.loadSettings()
  await deviceStore.loadPairedDevices()
  await network.loadLocalAddresses()

  // 如果配置中没有 qr_host，自动选择一个合适的 IP
  if (!settingsStore.settings.network.qr_host && ipv4Addresses.value.length > 0) {
    selectedIp.value = ipv4Addresses.value[0]
    await settingsStore.saveSettings({
      network: { ...settingsStore.settings.network, qr_host: selectedIp.value },
    })
  } else {
    selectedIp.value = qrHost.value || null
  }

  // 加载初始在线设备列表
  await refreshDevices()

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

  // 监听设备连接事件
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

  // 监听设备断开事件
  deviceDisconnectedListener = await listen<DeviceConnectionInfo>('device-disconnected', (event) => {
    const fp = (event.payload as any).fingerprint
    if (fp) {
      const newSet = new Set(connectedFingerprints.value)
      newSet.delete(fp)
      connectedFingerprints.value = newSet
    }
  })

  // 监听配对码自动生成事件（移动端发起配对请求时后端生成）
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
    },
  )
})

onUnmounted(() => {
  if (countdownInterval) {
    clearInterval(countdownInterval)
  }
  if (pairingCodeListener) pairingCodeListener()
  if (deviceConnectedListener) deviceConnectedListener()
  if (deviceDisconnectedListener) deviceDisconnectedListener()
  if (qrTokenConsumedListener) qrTokenConsumedListener()
  // 不清除 QR 码和配对码，保持状态以便下次进入页面时恢复
})

/** 配对码倒计时 */
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

/** 生成配对码（后端真实生成） */
async function generateCode() {
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

/** 取消配对码 */
function cancelPairing() {
  pairing.clearCode()
  pairingCode.value = null
  remainingSeconds.value = 0
  if (countdownInterval) {
    clearInterval(countdownInterval)
    countdownInterval = null
  }
}

function removeDevice(deviceId: string) {
  pendingDeviceId.value = deviceId
  showRemoveDeviceDialog.value = true
}

function viewHistory(deviceId: string) {
  router.push(`/devices/${deviceId}/history`)
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
