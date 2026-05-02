<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3 flex items-center justify-between">
      <h1 class="text-lg font-semibold">设备</h1>
      <button
        class="p-2 rounded-lg bg-dark-700 text-dark-300"
        @click="handleScan"
        :disabled="connection.state.value.status === 'connecting'"
      >
        <svg
          :class="['w-5 h-5', connection.state.value.status === 'connecting' && 'animate-spin']"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
    </header>

    <!-- Device List -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Discovered Devices -->
      <div class="mb-6">
        <h3 class="text-dark-400 text-sm font-medium mb-3 flex items-center gap-2">
          <span>发现设备</span>
          <span v-if="connection.state.value.status === 'connecting'" class="text-primary-400">扫描中...</span>
        </h3>

        <div v-if="connection.discoveredDevices.value.length === 0" class="text-center py-12">
          <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-dark-800 flex items-center justify-center">
            <svg class="w-8 h-8 text-dark-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
            </svg>
          </div>
          <p class="text-dark-500 text-sm">
            {{ connection.state.value.status === 'connecting' ? '正在搜索附近设备...' : '点击右上角扫描设备' }}
          </p>
        </div>

        <div v-else class="space-y-2">
          <DeviceCard
            v-for="device in connection.discoveredDevices.value"
            :key="device.id"
            :device="{
              id: device.id,
              name: device.name,
              isOnline: true
            }"
            @click="handleConnect(device)"
          />
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

    <!-- Manual Connect Button -->
    <div class="p-4 border-t border-dark-700">
      <button
        class="w-full bg-dark-700 text-dark-200 py-3 rounded-xl font-medium active:bg-dark-600"
        @click="showManualConnect = true"
      >
        手动输入地址连接
      </button>
    </div>

    <!-- Manual Connect Dialog -->
    <BottomSheet
      v-model="showManualConnect"
      title="手动连接"
      placeholder="输入设备地址 (如: 192.168.1.100)"
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
import { ref, onMounted } from 'vue'
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

// Current device being connected
const pendingDevice = ref<RemoteDevice | null>(null)

onMounted(async () => {
  await connection.loadPairedDevices()
})

async function handleScan() {
  await connection.discoverDevices()
}

function handleConnect(device: RemoteDevice) {
  pendingDevice.value = device
  showPairing.value = true
}

async function handleConnectManual(address: string) {
  const [host, portStr] = address.split(':')
  const port = portStr ? parseInt(portStr) : 8765

  pendingDevice.value = {
    id: `${host}:${port}`,
    name: host,
    address: host,
    port,
    isPaired: false,
  }
  showPairing.value = true
}

async function handlePairingSubmit(code: string) {
  if (!pendingDevice.value) return

  isPairing.value = true
  pairingError.value = ''

  try {
    // 连接到设备
    await connection.connect(pendingDevice.value)

    // 验证配对码
    const success = await connection.verifyPairingCode(code)

    if (success) {
      showPairing.value = false
      pendingDevice.value = null

      // 跳转到终端
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
