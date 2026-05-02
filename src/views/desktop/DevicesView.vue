<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-6 py-3 h-12 flex items-center">
      <h2 class="text-lg font-semibold">设备配对</h2>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <!-- Pairing Section -->
      <div class="bg-dark-800 rounded-lg border border-dark-700 p-6 mb-6">
        <h3 class="text-lg font-medium mb-4">新建配对</h3>

        <div v-if="!pairingCode" class="text-center py-4">
          <p class="text-dark-400 mb-4">生成配对码以连接移动设备</p>
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
          <p class="text-dark-400 mb-4">请移动端输入以下配对码</p>

          <!-- Pairing Code Display -->
          <div class="text-5xl font-mono font-bold text-primary-400 tracking-widest mb-4">
            {{ pairingCode.code }}
          </div>

          <p class="text-dark-500 text-sm mb-6">
            配对码将在 <span class="text-primary-400 font-medium">{{ remainingSeconds }}</span> 秒后过期
          </p>

          <Button variant="ghost" size="sm" @click="cancelPairing">
            取消
          </Button>
        </div>
      </div>

      <!-- Network Info -->
      <div class="bg-dark-800 rounded-lg border border-dark-700 p-6 mb-6">
        <h3 class="text-lg font-medium mb-4">网络信息</h3>
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-dark-400">端口</span>
            <span class="font-mono">8765</span>
          </div>
          <div class="flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <span class="text-dark-400">IPv4 地址</span>
              <div class="flex items-center gap-2 flex-wrap justify-end">
                <span v-for="ip in ipv4Addresses" :key="ip" class="font-mono text-sm bg-dark-700 px-2 py-1 rounded">
                  {{ ip }}
                </span>
                <span v-if="ipv4Addresses.length === 0" class="text-dark-500 text-sm">无</span>
              </div>
            </div>
            <div class="flex items-center justify-between">
              <span class="text-dark-400">IPv6 地址</span>
              <div class="flex items-center gap-2 flex-wrap justify-end">
                <span v-for="ip in ipv6Addresses" :key="ip" class="font-mono text-sm bg-dark-700 px-2 py-1 rounded">
                  {{ ip }}
                </span>
                <span v-if="ipv6Addresses.length === 0" class="text-dark-500 text-sm">无</span>
              </div>
            </div>
            <div class="flex items-center justify-between">
              <span class="text-dark-400">子网掩码</span>
              <div class="flex items-center gap-2 flex-wrap justify-end">
                <span v-for="mask in subnetMasks" :key="mask" class="font-mono text-sm bg-dark-700 px-2 py-1 rounded">
                  {{ mask }}
                </span>
                <span v-if="subnetMasks.length === 0" class="text-dark-500 text-sm">无</span>
              </div>
            </div>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-dark-400">mDNS 发现</span>
            <Toggle v-model="mDnsEnabled" @update:model-value="toggleMDns" />
          </div>
        </div>
      </div>

      <!-- Paired Devices -->
      <div class="bg-dark-800 rounded-lg border border-dark-700 p-6">
        <h3 class="text-lg font-medium mb-4">已配对设备</h3>

        <div v-if="deviceStore.pairedDevices.length === 0" class="text-center py-8">
          <svg class="w-12 h-12 mx-auto text-dark-600 mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
          <p class="text-dark-400">暂无已配对设备</p>
        </div>

        <div v-else class="space-y-3">
          <div
            v-for="device in deviceStore.pairedDevices"
            :key="device.id"
            class="flex items-center justify-between p-4 bg-dark-700 rounded-lg"
          >
            <div class="flex items-center gap-4">
              <!-- Status Indicator -->
              <div
                :class="[
                  'w-3 h-3 rounded-full',
                  device.isActive ? 'bg-green-500' : 'bg-dark-500'
                ]"
              ></div>

              <div>
                <p class="font-medium">{{ device.deviceName }}</p>
                <p class="text-dark-400 text-sm">
                  配对于 {{ formatDate(device.pairedAt) }}
                </p>
              </div>
            </div>

            <div class="flex items-center gap-3">
              <span
                :class="[
                  'text-xs px-2 py-1 rounded',
                  device.isActive ? 'bg-green-900/50 text-green-300' : 'bg-dark-600 text-dark-400'
                ]"
              >
                {{ device.isActive ? '在线' : '离线' }}
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
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useDeviceStore } from '@/stores/device'
import { usePairing, useNetwork, useDiscovery } from '@/composables/useTauri'
import Button from '@/components/common/Button.vue'
import Toggle from '@/components/common/Toggle.vue'
import { useToast } from '@/composables/useToast'

const deviceStore = useDeviceStore()
const pairing = usePairing()
const network = useNetwork()
const discovery = useDiscovery()
const toast = useToast()

const isLoading = ref(false)
const pairingCode = ref<{ code: string; expiresIn: number } | null>(null)
const remainingSeconds = ref(0)
const mDnsEnabled = ref(true)

const localAddresses = computed(() => network.localAddresses.value)

// 分类 IP 地址
const ipv4Addresses = computed(() => {
  return localAddresses.value.filter(ip => ip.includes('.'))
})

const ipv6Addresses = computed(() => {
  return localAddresses.value.filter(ip => ip.includes(':'))
})

// 子网掩码（根据 IPv4 地址推断常用掩码）
const subnetMasks = computed(() => {
  // 常见私有网络的子网掩码
  const masks: string[] = []
  ipv4Addresses.value.forEach(ip => {
    if (ip.startsWith('192.168.')) {
      if (!masks.includes('255.255.255.0')) masks.push('255.255.255.0')
    } else if (ip.startsWith('10.')) {
      if (!masks.includes('255.0.0.0')) masks.push('255.0.0.0')
    } else if (ip.startsWith('172.')) {
      const second = parseInt(ip.split('.')[1])
      if (second >= 16 && second <= 31) {
        if (!masks.includes('255.240.0.0')) masks.push('255.240.0.0')
      }
    }
  })
  return masks
})

let countdownInterval: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await deviceStore.loadPairedDevices()
  await network.loadLocalAddresses()

  // Start mDNS broadcast
  if (mDnsEnabled.value) {
    await discovery.startBroadcast('BedCode', 8765)
  }
})

onUnmounted(() => {
  if (countdownInterval) {
    clearInterval(countdownInterval)
  }
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
  if (confirm('确定要移除此设备吗？移除后需要重新配对。')) {
    await deviceStore.removeDevice(deviceId)
    toast.success('设备已移除')
  }
}

async function toggleMDns(enabled: boolean) {
  if (enabled) {
    await discovery.startBroadcast('BedCode', 8765)
    toast.success('mDNS 发现已启用')
  } else {
    toast.info('mDNS 发现已禁用')
  }
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
