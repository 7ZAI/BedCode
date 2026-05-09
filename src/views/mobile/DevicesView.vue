<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header with safe area padding -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3" style="padding-top: calc(var(--safe-area-inset-top, 0px) + 12px);">
      <h1 class="text-lg font-semibold">会话配置</h1>
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

    <!-- Connected Banner -->
    <div
      v-if="isConnected && connection.currentDevice.value"
      class="mx-4 mt-4 p-3 bg-green-900/20 border border-green-800/30 rounded-lg"
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="w-3 h-3 rounded-full bg-green-500"></div>
          <div>
            <p class="text-green-300 text-sm font-medium">{{ connection.currentDevice.value.name }}</p>
            <p class="text-green-500/70 text-xs">{{ connection.currentDevice.value.address }}</p>
          </div>
        </div>
        <button
          class="px-3 py-1.5 bg-dark-700 text-dark-300 text-sm rounded-lg"
          @click="handleDisconnect"
        >
          断开
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Session Configs (when connected) -->
      <div v-if="isConnected">
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-dark-400 text-sm font-medium">会话配置</h3>
          <button
            class="text-dark-500 text-xs"
            :class="{ 'opacity-50': isRefreshing }"
            :disabled="isRefreshing"
            @click="refreshConfigs"
          >
            {{ isRefreshing ? '刷新中...' : '刷新' }}
          </button>
        </div>

        <!-- Loading -->
        <div v-if="isLoadingConfigs" class="text-center py-12">
          <div class="w-8 h-8 border-2 border-primary-400 border-t-transparent rounded-full animate-spin mx-auto mb-3" />
          <p class="text-dark-500 text-sm">加载配置中...</p>
        </div>

        <!-- Empty -->
        <div v-else-if="sessionConfigs.length === 0" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-dark-400">暂无会话配置</p>
          <p class="text-dark-500 text-sm mt-2">请在桌面端创建会话配置</p>
        </div>

        <!-- Config List -->
        <div v-else class="space-y-2">
          <div
            v-for="config in sessionConfigs"
            :key="config.id"
            class="bg-dark-800 rounded-xl p-4 active:bg-dark-700 transition-colors"
            @click="handleStartSession(config)"
          >
            <div class="flex items-start justify-between">
              <div class="flex-1 min-w-0">
                <p class="font-medium">{{ config.name }}</p>
                <div class="flex items-center gap-2 mt-1.5">
                  <span
                    :class="[
                      'text-xs px-2 py-0.5 rounded-full',
                      config.environment === 'wsl2' ? 'bg-purple-900/50 text-purple-400' : 'bg-blue-900/50 text-blue-400'
                    ]"
                  >
                    {{ config.environment === 'wsl2' ? 'WSL2' : 'Windows' }}
                  </span>
                  <span v-if="config.wsl_distro" class="text-dark-500 text-xs">{{ config.wsl_distro }}</span>
                </div>
                <p class="text-dark-400 text-sm mt-1 truncate">{{ config.command }}</p>
                <p class="text-dark-500 text-xs mt-0.5 truncate">{{ config.working_dir }}</p>
              </div>
              <div class="flex items-center ml-3">
                <div
                  v-if="startingConfigId === config.id"
                  class="w-5 h-5 border-2 border-primary-400 border-t-transparent rounded-full animate-spin"
                />
                <svg v-else class="w-5 h-5 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                </svg>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Connection History (when not connected) -->
      <div v-else>
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
    </div>

    <!-- Action Buttons (when not connected) -->
    <div v-if="!isConnected" class="p-4 border-t border-dark-700 space-y-3">
      <!-- Scan QR Code Button -->
      <button
        class="w-full bg-dark-700 text-white py-3 rounded-xl font-medium active:bg-dark-600 flex items-center justify-center gap-2"
        :class="{ 'opacity-50': isConnecting }"
        :disabled="isConnecting"
        @click="$router.push({ name: 'mobile-scan' })"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
        </svg>
        扫描连接
      </button>

      <!-- Manual Connect Button -->
      <button
        class="w-full bg-primary-600 text-white py-3 rounded-xl font-medium active:bg-primary-700 flex items-center justify-center gap-2"
        :class="{ 'opacity-50': isConnecting }"
        :disabled="isConnecting"
        @click="showManualConnect = true"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
        </svg>
        手动连接
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
import BottomSheet from '@/components/mobile/BottomSheet.vue'
import PairingInput from '@/components/mobile/PairingInput.vue'
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'

const router = useRouter()
const connection = useRemoteConnection()

const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
})

const showManualConnect = ref(false)
const showPairing = ref(false)
const isPairing = ref(false)
const pairingError = ref('')
const isConnecting = ref(false)
const connectionError = ref('')

// Session configs from connected desktop
interface SessionConfigSummary {
  id: string
  name: string
  environment: string
  wsl_distro?: string
  working_dir: string
  command: string
}
const sessionConfigs = ref<SessionConfigSummary[]>([])
const isLoadingConfigs = ref(false)
const isRefreshing = ref(false)
const startingConfigId = ref<string | null>(null)

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

const isConnected = computed(() => connection.state.value.status === 'paired' && connection.isConnected.value)

const connectionStatusText = computed(() => {
  switch (connectionStatus.value) {
    case 'connecting':
      return `正在连接 ${pendingDevice.value?.name || '连接'}...`
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

function saveConnectionHistory() {
  localStorage.setItem('connection_history', JSON.stringify(connectionHistory.value))
}

function addToHistory(address: string, name?: string) {
  connectionHistory.value = connectionHistory.value.filter(item => item.address !== address)
  connectionHistory.value.unshift({
    address,
    name: name || address.split(':')[0],
    lastConnected: new Date().toISOString(),
  })
  if (connectionHistory.value.length > 10) {
    connectionHistory.value = connectionHistory.value.slice(0, 10)
  }
  saveConnectionHistory()
}

function removeFromHistory(address: string) {
  connectionHistory.value = connectionHistory.value.filter(item => item.address !== address)
  saveConnectionHistory()
}

function clearHistory() {
  connectionHistory.value = []
  saveConnectionHistory()
}

// Load session configs from connected desktop
async function loadSessionConfigs() {
  if (!isConnected.value) return

  isLoadingConfigs.value = true
  try {
    const response = await connection.sendMessageWithResponse('control', {
      action: { type: 'list_session_configs' },
    })

    if (response?.payload?.action?.type === 'session_config_list') {
      sessionConfigs.value = response.payload.action.configs.map((c: any) => ({
        id: c.id,
        name: c.name,
        environment: c.environment,
        wsl_distro: c.wsl_distro,
        working_dir: c.working_dir,
        command: c.command,
      }))
    }
  } catch (e) {
    console.error('Failed to load session configs:', e)
  } finally {
    isLoadingConfigs.value = false
  }
}

async function refreshConfigs() {
  isRefreshing.value = true
  await loadSessionConfigs()
  isRefreshing.value = false
}

// Start session from config
async function handleStartSession(config: SessionConfigSummary) {
  if (!isConnected.value || startingConfigId.value) return

  startingConfigId.value = config.id
  try {
    const response = await connection.sendMessageWithResponse('control', {
      action: { type: 'start_session', config_id: config.id },
    })

    const sessionId = response?.session_id
    if (sessionId) {
      // Refresh active sessions for the Sessions tab
      await terminal.loadSessions()
      connection.activeSessionId.value = sessionId
    } else {
      console.error('Failed to start session: no session_id in response')
    }
  } catch (e) {
    console.error('Failed to start session:', e)
  } finally {
    startingConfigId.value = null
  }
}

onMounted(async () => {
  loadConnectionHistory()

  // If already connected, load session configs and active sessions
  if (isConnected.value) {
    await loadSessionConfigs()
    await terminal.loadSessions()
  }
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

    // Step 2: Try stored credentials first (for reconnection without re-pairing)
    const authenticated = await connection.authenticate()
    if (authenticated) {
      // Successfully reconnected, load configs
      connectionStatus.value = 'idle'
      pendingDevice.value = null
      addToHistory(`${device.address}:${device.port}`, device.name)
      await loadSessionConfigs()
      return
    }

    // Step 3: Need to pair
    await connection.requestPairing()
    connectionStatus.value = 'pairing'
    showPairing.value = true
    addToHistory(`${device.address}:${device.port}`, device.name)
  } catch (error) {
    connectionStatus.value = 'error'
    connectionError.value = String(error)
    console.error('Connection failed:', error)
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

      // Load session configs instead of navigating to terminal
      await loadSessionConfigs()
      // Also fetch active sessions for the Sessions tab
      await terminal.loadSessions()
    } else {
      pairingError.value = '配对码验证失败，请重试'
    }
  } catch (error) {
    pairingError.value = String(error)
  } finally {
    isPairing.value = false
  }
}

function handleDisconnect() {
  connection.disconnect()
  sessionConfigs.value = []
}
</script>
