<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-6 py-3 h-12 flex items-center">
      <h2 class="text-lg font-semibold">设备配对</h2>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <!-- Pairing Section -->
      <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6 mb-6">
        <h3 class="text-lg font-medium mb-4">新建配对</h3>

        <div v-if="!pairingCode" class="text-center py-4">
          <p class="text-gray- dark:text-dark-400 mb-4">生成配对码以连接移动设备</p>
          <Button variant="primary" @click="generateCode" :loading="isLoading">
            <template #icon>
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
              </svg>
            </template>
            生成配对码
          </Button>
        </div>

        <div v-else class="text-center py-4">
          <p class="text-gray- dark:text-dark-400 mb-4">请移动端输入以下配对码</p>

          <!-- Pairing Code Display -->
          <div class="text-5xl font-mono font-bold text-primary-400 tracking-widest mb-4">
            {{ pairingCode.code }}
          </div>

          <p class="text-gray- dark:text-dark-500 text-sm mb-6">
            配对码将在 <span class="text-primary-400 font-medium">{{ remainingSeconds }}</span> 秒后过期
          </p>

          <Button variant="ghost" size="sm" @click="cancelPairing">
            取消
          </Button>
        </div>
      </div>

      <!-- QR Code Section -->
      <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6 mb-6">
        <h3 class="text-lg font-medium mb-4">QR 码连接</h3>

        <div v-if="!qr.hasQr.value" class="text-center py-4">
          <p class="text-gray- dark:text-dark-400 mb-4">扫描二维码快速连接移动设备</p>
          <Button variant="secondary" @click="qr.generateQr(selectedIp || undefined)" :loading="qr.isLoading.value">
            <template #icon>
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
              </svg>
            </template>
            生成二维码
          </Button>
        </div>

        <div v-else class="text-center py-4">
          <p class="text-gray- dark:text-dark-300 mb-4">使用移动端 BedCode 扫描二维码</p>

          <!-- QR Code Canvas -->
          <div class="inline-block bg-white p-4 rounded-lg mb-4">
            <canvas ref="qrCanvasRef" class="w-48 h-48"></canvas>
          </div>

          <p class="text-gray- dark:text-dark-500 text-sm mb-4">
            二维码有效期
            <span class="text-primary-400 font-medium">{{ qr.remainingSeconds.value }}</span> 秒
          </p>

          <div class="flex items-center justify-center gap-3">
            <Button variant="ghost" size="sm" @click="qr.clearQr()">
              取消
            </Button>
            <Button variant="ghost" size="sm" @click="qr.generateQr(selectedIp || undefined)" :loading="qr.isLoading.value">
              刷新
            </Button>
          </div>
        </div>
      </div>

      <!-- Network Info -->
      <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6 mb-6">
        <h3 class="text-lg font-medium mb-4">网络信息</h3>
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-gray- dark:text-dark-400">WebSocket 端口</span>
            <span class="font-mono">{{ port }}</span>
          </div>
          <div class="flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <span class="text-gray- dark:text-dark-400">IPv4 地址</span>
              <div class="flex items-center gap-2">
                <span class="font-mono text-sm bg-gray-100 dark:bg-dark-700 px-2 py-1 rounded">
                  {{ displayIp }}
                </span>
                <button
                  @click="showIpSelector = true"
                  class="text-sm text-primary-400 hover:text-primary-300"
                >
                  选择
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- IP Selector Modal -->
      <Modal v-model="showIpSelector" title="选择 IP 地址">
        <div class="space-y-2">
          <p class="text-sm text-gray- dark:text-dark-400 mb-4">选择移动设备可访问的 IP 地址</p>
          <div
            v-for="ip in ipv4Addresses"
            :key="ip"
            @click="selectIp(ip)"
            :class="[
              'p-3 rounded-lg cursor-pointer border transition-colors',
              selectedIp === ip
                ? 'border-primary-400 bg-primary-400/10'
                : 'border-gray-200 dark:border-dark-600 hover:border-primary-300'
            ]"
          >
            <span class="font-mono">{{ ip }}</span>
          </div>
          <p v-if="ipv4Addresses.length === 0" class="text-gray- dark:text-dark-500 text-center py-4">
            未找到可用的 IPv4 地址
          </p>
        </div>
        <div class="mt-4 flex justify-end">
          <Button variant="ghost" @click="showIpSelector = false">取消</Button>
        </div>
      </Modal>

      <!-- Paired Devices -->
      <div class="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-6">
        <h3 class="text-lg font-medium mb-4">已配对设备</h3>

        <div v-if="deviceStore.pairedDevices.length === 0" class="text-center py-8">
          <svg class="w-12 h-12 mx-auto text-gray- dark:text-dark-600 mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
          <p class="text-gray- dark:text-dark-400">暂无已配对设备</p>
        </div>

        <div v-else class="space-y-3">
          <div
            v-for="device in deviceStore.pairedDevices"
            :key="device.id"
            class="flex items-center justify-between p-4 bg-gray-100 dark:bg-dark-700 rounded-lg"
          >
            <div class="flex items-center gap-4">
              <!-- Status Indicator (live WebSocket status) -->
              <div
                :class="[
                  'w-3 h-3 rounded-full',
                  isDeviceOnline(device.id) ? 'bg-green-500 animate-pulse' : 'bg-dark-500'
                ]"
              ></div>

              <div>
                <p class="font-medium">{{ device.deviceName }}</p>
                <p class="text-gray- dark:text-dark-400 text-sm">
                  配对于 {{ formatDate(device.pairedAt) }}
                </p>
              </div>
            </div>

            <div class="flex items-center gap-3">
              <span
                :class="[
                  'text-xs px-2 py-1 rounded',
                  isDeviceOnline(device.id) ? 'bg-green-900/50 text-green-300' : 'bg-gray-200 dark:bg-dark-600 text-gray- dark:text-dark-400'
                ]"
              >
                {{ isDeviceOnline(device.id) ? '已连接' : '离线' }}
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
    <Modal v-model="showRemoveDeviceDialog" title="确认移除" size="sm">
      <p class="text-gray- dark:text-dark-300">确定要移除此设备吗？移除后需要重新配对。</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showRemoveDeviceDialog = false">取消</Button>
          <Button variant="danger" @click="confirmRemoveDevice">移除</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useDeviceStore } from '@/stores/device'
import { useSettingsStore } from '@/stores/settings'
import { usePairing, useNetwork, useConnectedDevices, type DeviceConnectionInfo } from '@/composables/useTauri'
import { useQrCode } from '@/composables/useQrCode'
import { listen } from '@tauri-apps/api/event'
import Button from '@/components/common/Button.vue'
import Modal from '@/components/common/Modal.vue'
import { useToast } from '@/composables/useToast'
import QRCode from 'qrcode'

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
const displayIp = computed(() => qrHost.value || '未选择')

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

// Real-time connected device IDs (from WebSocket events)
const connectedDeviceIds = ref<Set<string>>(new Set())

// 检查设备是否实时在线
function isDeviceOnline(deviceId: string): boolean {
  return connectedDeviceIds.value.has(deviceId)
}

const isLoading = ref(false)
const showIpSelector = ref(false)
const pairingCode = ref<{ code: string; expiresIn: number } | null>(null)
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
  const ids = new Set(connected.connectedDevices.value.map(d => d.device_id))
  connectedDeviceIds.value = ids

  // Listen for real-time device connection events
  deviceConnectedListener = await listen<DeviceConnectionInfo>('device-connected', (event) => {
    const deviceId = event.payload.device_id
    connectedDeviceIds.value = new Set([...connectedDeviceIds.value, deviceId])

    // 当有设备连接成功后，自动刷新二维码（token 已被消费）
    if (qr.hasQr.value) {
      console.log('Device connected, refreshing QR code...')
      qr.generateQr(selectedIp.value || undefined)
    }

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
  deviceDisconnectedListener = await listen<DeviceConnectionInfo>('device-disconnected', (event) => {
    const newSet = new Set(connectedDeviceIds.value)
    newSet.delete(event.payload.device_id)
    connectedDeviceIds.value = newSet
  })

  // 监听配对码自动生成事件
  pairingCodeListener = await listen<{ code: string; expires_in: number; device_name?: string }>(
    'pairing-code-generated',
    (event) => {
      console.log('Received pairing-code-generated event:', event.payload)
      pairingCode.value = {
        code: event.payload.code,
        expiresIn: event.payload.expires_in,
      }
      remainingSeconds.value = event.payload.expires_in

      // 开始倒计时
      if (countdownInterval) {
        clearInterval(countdownInterval)
      }
      countdownInterval = setInterval(() => {
        if (remainingSeconds.value > 0) {
          remainingSeconds.value--
        } else {
          pairingCode.value = null
          if (countdownInterval) {
            clearInterval(countdownInterval)
            countdownInterval = null
          }
        }
      }, 1000)

      toast.info(`移动端请求配对，请输入配对码: ${event.payload.code}`)
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
  qr.clearQr()
})

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
      remainingSeconds.value = pairingCode.value.expiresIn || 60

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
    } else {
      toast.error('生成配对码失败：未收到有效配对码')
    }
  } catch (e) {
    console.error('生成配对码失败:', e)
    toast.error('生成配对码失败')
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
  toast.success('设备已移除')
  showRemoveDeviceDialog.value = false
  pendingDeviceId.value = null
}

function formatDate(dateStr: string): string {
  const date = new Date(dateStr)
  return date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
</script>
