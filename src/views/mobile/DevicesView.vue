<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header with safe area padding -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3" style="padding-top: calc(var(--safe-area-inset-top, 0px) + 12px);">
      <h1 class="text-lg font-semibold">设备</h1>
    </header>

    <!-- Connection Status Banner -->
    <div v-if="connectionStatus" class="px-4 py-3 bg-dark-800 border-b border-dark-700">
      <div class="flex items-center gap-3">
        <!-- Connecting spinner -->
        <div v-if="connectionStatus === 'connecting'" class="w-5 h-5 border-2 border-primary-400 border-t-transparent rounded-full animate-spin" />
        <!-- Success icon -->
        <svg v-else-if="connectionStatus === 'connected'" class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
        <!-- Error icon -->
        <svg v-else-if="connectionStatus === 'error'" class="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
        <!-- Pairing icon -->
        <div v-else-if="connectionStatus === 'pairing'" class="w-5 h-5 bg-primary-400 rounded-full flex items-center justify-center">
          <span class="text-xs text-dark-900 font-bold">?</span>
        </div>

        <span class="text-sm" :class="{
          'text-dark-300': connectionStatus === 'connecting',
          'text-green-400': connectionStatus === 'connected',
          'text-red-400': connectionStatus === 'error',
          'text-primary-400': connectionStatus === 'pairing',
        }">
          {{ connectionStatusText }}
        </span>
      </div>
    </div>

    <!-- Device List -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Connection History -->
      <div class="mb-6">
        <h3 class="text-dark-400 text-sm font-medium mb-3 flex items-center justify-between">
          <span>连接历史</span>
          <button
            v-if="connectionHistory.length > 0"
            class="text-dark-500 text-xs"
            @click="clearHistory"
          >
            清除
          </button>
        </h3>

        <div v-if="connectionHistory.length === 0" class="text-center py-8">
          <p class="text-dark-500 text-sm">暂无连接历史</p>
        </div>

        <div v-else class="space-y-2">
          <div
            v-for="item in connectionHistory"
            :key="item.address"
            class="flex items-center justify-between p-3 bg-dark-800 rounded-lg"
            @click="handleConnectFromHistory(item)"
          >
            <div class="flex items-center gap-3">
              <div class="w-10 h-10 rounded-full bg-dark-700 flex items-center justify-center">
                <svg class="w-5 h-5 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </div>
              <div>
                <p class="font-medium text-dark-200">{{ item.name || item.address }}</p>
                <p class="text-dark-500 text-xs">{{ item.address }}</p>
              </div>
            </div>
            <button
              class="p-2 text-dark-500 hover:text-red-400"
              @click.stop="removeFromHistory(item.address)"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </div>

      <!-- Paired Devices -->
      <div>
        <h3 class="text-dark-400 text-sm font-medium mb-3">已配对设备</h3>

        <div v-if="connection.pairedDevices.value.length === 0" class="text-center py-8">
          <p class="text-dark-500 text-sm">暂无已配对设备</p>
        </div>

        <div v-else class="space-y-2">
          <DeviceCard
            v-for="device in connection.pairedDevices.value"
            :key="device.id"
            :device="{
              id: device.id,
              name: device.name,
              isOnline: true
            }"
            @click="handleOpenTerminal(device)"
          />
        </div>
      </div>
    </div>

    <!-- Scan QR Code FAB -->
    <button
      class="fixed bottom-20 right-4 w-14 h-14 bg-primary-500 hover:bg-primary-600 rounded-full flex items-center justify-center shadow-lg shadow-primary-500/30 transition-all active:scale-95 z-10"
      @click="$router.push({ name: 'mobile-scan' })"
    >
      <svg class="w-7 h-7 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
      </svg>
    </button>

    <!-- Manual Connect Button -->
    <div class="p-4 border-t border-dark-700">
      <button
        class="w-full bg-primary-600 text-white py-3 rounded-xl font-medium active:bg-primary-700"
        :class="{ 'opacity-50': isConnecting }"
        :disabled="isConnecting"
        @click="showManualConnect = true"
      >
        连接新设备
      </button>
    </div>

    <!-- Manual Connect Dialog -->
    <BottomSheet
      v-model="showManualConnect"
      title="连接新设备"
      placeholder="输入设备地址 (如: 10.186.131.120:8765)"
      @submit="handleConnectManual"
    />

    <!-- Pairing Dialog -->
    <PairingInput
      v-model="showPairing"
      :loading="isPairing"
      :error="pairingError"
      @submit="handlePairingSubmit"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useRemoteConnection, type RemoteDevice } from '@/composables/useRemoteConnection'
import DeviceCard from '@/components/mobile/DeviceCard.vue'
import BottomSheet from '@/components/mobile/BottomSheet.vue'
import PairingInput from '@/components/mobile/PairingInput.vue'

const router = useRouter()
const connection = useRemoteConnection()

const showManualConnect = ref(false)
const showPairing = ref(false)
const isPairing = ref(false)
const pairingError = ref('')
const isConnecting = ref(false)
const connectionError = ref('')

// Connection history (stored in localStorage)
interface ConnectionHistoryItem {
  address: string
  name: string
  lastConnected: string
}
const connectionHistory = ref<ConnectionHistoryItem[]>([])

// Current device being connected
const pendingDevice = ref<RemoteDevice | null>(null)

// Connection status for UI display
const connectionStatus = ref<'idle' | 'connecting' | 'connected' | 'pairing' | 'error'>('idle')

const connectionStatusText = computed(() => {
  switch (connectionStatus.value) {
    case 'connecting':
      return `正在连接 ${pendingDevice.value?.name || '设备'}...`
    case 'connected':
      return '已连接，正在请求配对...'
    case 'pairing':
      return '请在桌面端查看 6 位配对码并输入'
    case 'error':
      return connectionError.value || '连接失败'
    default:
      return ''
  }
})

// Load connection history from localStorage
function loadConnectionHistory() {
  const stored = localStorage.getItem('connection_history')
  if (stored) {
    try {
      connectionHistory.value = JSON.parse(stored)
    } catch {
      connectionHistory.value = []
    }
  }
}

// Save connection history to localStorage
function saveConnectionHistory() {
  localStorage.setItem('connection_history', JSON.stringify(connectionHistory.value))
}

// Add to connection history
function addToHistory(address: string, name?: string) {
  // Remove existing entry with same address
  connectionHistory.value = connectionHistory.value.filter(item => item.address !== address)

  // Add new entry at the beginning
  connectionHistory.value.unshift({
    address,
    name: name || address.split(':')[0],
    lastConnected: new Date().toISOString(),
  })

  // Keep only last 10 entries
  if (connectionHistory.value.length > 10) {
    connectionHistory.value = connectionHistory.value.slice(0, 10)
  }

  saveConnectionHistory()
}

// Remove from connection history
function removeFromHistory(address: string) {
  connectionHistory.value = connectionHistory.value.filter(item => item.address !== address)
  saveConnectionHistory()
}

// Clear all history
function clearHistory() {
  connectionHistory.value = []
  saveConnectionHistory()
}

onMounted(async () => {
  await connection.loadPairedDevices()
  loadConnectionHistory()
})

// Connect from history
async function handleConnectFromHistory(item: ConnectionHistoryItem) {
  const [host, portStr] = item.address.split(':')
  const port = portStr ? parseInt(portStr) : 8765

  const device: RemoteDevice = {
    id: `${host}:${port}`,
    name: item.name,
    address: host,
    port,
    isPaired: false,
  }

  await startConnection(device)
}

// Manual address input
async function handleConnectManual(address: string) {
  // Parse address
  const [host, portStr] = address.split(':')
  const port = portStr ? parseInt(portStr) : 8765

  const device: RemoteDevice = {
    id: `${host}:${port}`,
    name: host,
    address: host,
    port,
    isPaired: false,
  }

  await startConnection(device)
}

// Start connection flow
async function startConnection(device: RemoteDevice) {
  pendingDevice.value = device
  connectionStatus.value = 'connecting'
  connectionError.value = ''
  isConnecting.value = true

  try {
    // Step 1: Connect to device
    await connection.connect(device)
    connectionStatus.value = 'connected'

    // Step 2: Request pairing
    await connection.requestPairing()
    connectionStatus.value = 'pairing'

    // Step 3: Show pairing input dialog
    showPairing.value = true

    // Add to history after successful connection
    addToHistory(`${device.address}:${device.port}`, device.name)
  } catch (error) {
    connectionStatus.value = 'error'
    connectionError.value = String(error)
    console.error('Connection failed:', error)

    // Clear error after 3 seconds
    setTimeout(() => {
      if (connectionStatus.value === 'error') {
        connectionStatus.value = 'idle'
      }
    }, 3000)
  } finally {
    isConnecting.value = false
  }
}

// Verify pairing code
async function handlePairingSubmit(code: string) {
  if (!pendingDevice.value) return

  isPairing.value = true
  pairingError.value = ''

  try {
    const success = await connection.verifyPairingCode(code)

    if (success) {
      showPairing.value = false
      connectionStatus.value = 'idle'
      pendingDevice.value = null

      // Navigate to terminal
      router.push(`/mobile/terminal/${connection.currentDevice.value?.id}`)
    } else {
      pairingError.value = '配对码验证失败，请重试'
    }
  } catch (error) {
    pairingError.value = String(error)
  } finally {
    isPairing.value = false
  }
}

function handleOpenTerminal(device: RemoteDevice) {
  router.push(`/mobile/terminal/${device.id}`)
}
</script>