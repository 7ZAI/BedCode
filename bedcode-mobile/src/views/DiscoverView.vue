<template>
  <div class="h-full w-full bg-[var(--mobile-bg-primary)] flex flex-col">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center gap-3">
      <button
        class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-cyan-500/10 transition-colors"
        @click="goBack"
      >
        <svg class="w-5 h-5 text-[var(--mobile-text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h2 class="text-lg font-semibold text-[var(--mobile-text-primary)]">{{ t('mobile.discover.title') }}</h2>
    </header>

    <!-- Scanning status -->
    <div v-if="isScanning" class="px-4 py-3 bg-[var(--mobile-bg-secondary)] border-b border-[var(--mobile-border)] flex items-center gap-3">
      <div class="w-5 h-5 border-2 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin" />
      <span class="text-sm text-[var(--mobile-text-muted)]">{{ t('mobile.discover.scanning') }}</span>
      <span v-if="discoveredServices.length > 0" class="text-sm text-[var(--mobile-accent)] ml-auto">
        {{ t('mobile.discover.deviceFound', { count: discoveredServices.length }) }}
      </span>
    </div>

    <!-- Device list -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Empty state -->
      <div v-if="discoveredServices.length === 0 && !isScanning" class="flex flex-col items-center justify-center h-full">
        <svg class="w-16 h-16 text-[var(--mobile-accent)]/30 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.858 15.355-5.858 21.213 0" />
        </svg>
        <p class="text-[var(--mobile-text-muted)] mb-1">{{ t('mobile.discover.noDevices') }}</p>
        <p class="text-[var(--mobile-text-disabled)] text-sm">{{ t('mobile.discover.noDevicesHint') }}</p>
      </div>

      <!-- Scanning empty state (show animation) -->
      <div v-if="discoveredServices.length === 0 && isScanning" class="flex flex-col items-center justify-center h-full">
        <div class="relative mb-6">
          <!-- Radar animation -->
          <div class="w-32 h-32 rounded-full border-2 border-[var(--mobile-accent)]/20 relative">
            <div class="absolute inset-2 rounded-full border border-[var(--mobile-accent)]/10"></div>
            <div class="absolute inset-4 rounded-full border border-[var(--mobile-accent)]/5"></div>
            <!-- Sweep line -->
            <div class="absolute inset-0 animate-[spin_3s_linear_infinite] origin-center">
              <div class="w-1/2 h-0.5 bg-gradient-to-r from-[var(--mobile-accent)] to-transparent absolute top-1/2 left-1/2 -translate-y-1/2"></div>
            </div>
            <!-- Center dot -->
            <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-3 h-3 rounded-full bg-[var(--mobile-accent)]"></div>
          </div>
        </div>
        <p class="text-[var(--mobile-text-muted)]">{{ t('mobile.discover.scanning') }}</p>
      </div>

      <!-- Device cards -->
      <div v-if="discoveredServices.length > 0" class="space-y-2">
        <div
          v-for="service in discoveredServices"
          :key="service.instance_name"
          class="p-4 bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl shadow-[var(--mobile-card-shadow)] hover:border-[var(--mobile-border-active)] hover:shadow-[var(--mobile-card-shadow-hover)] transition-all cursor-pointer"
          @click="handleConnect(service)"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
              <div class="w-10 h-10 rounded-full bg-[var(--mobile-accent-muted)] border border-[var(--mobile-border-hover)] flex items-center justify-center">
                <svg class="w-5 h-5 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </div>
              <div>
                <p class="font-medium text-[var(--mobile-text-primary)]">{{ service.device_name }}</p>
                <p class="text-[var(--mobile-text-disabled)] text-xs">{{ service.address }}:{{ service.port }}</p>
              </div>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-xs px-2 py-0.5 rounded-full bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]">{{ service.platform }}</span>
              <svg class="w-4 h-4 text-[var(--mobile-text-disabled)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Bottom actions -->
    <div class="p-4 border-t border-[var(--mobile-border)] pb-safe">
      <button
        v-if="isScanning"
        class="w-full py-3 bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-text-secondary)] rounded-xl font-medium hover:bg-[var(--mobile-accent-muted)] transition-all"
        @click="stopScan"
      >
        {{ t('mobile.discover.stopScan') }}
      </button>
      <button
        v-else
        class="w-full py-3 bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] rounded-xl font-medium hover:bg-[var(--mobile-accent)]/30 transition-all"
        @click="startScan"
      >
        {{ t('mobile.discover.restartScan') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 设备发现页面 - 通过 mDNS 扫描局域网内的 BedCode 桌面端
 */
import { onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMdnsDiscovery, type DiscoveredService } from '@/composables/useMdnsDiscovery'
import { useMobileConnection, type RemoteDevice } from '@/composables/useMobileConnection'

const router = useRouter()
const connection = useMobileConnection()
const { discoveredServices, isScanning, startDiscovery, stopDiscovery } = useMdnsDiscovery()
const { t } = useI18n()

function goBack() {
  router.back()
}

async function startScan() {
  try {
    await startDiscovery()
  } catch (e) {
    console.error('[DiscoverView] Start scan failed:', e)
  }
}

async function stopScan() {
  await stopDiscovery()
}

function handleConnect(service: DiscoveredService) {
  // 检查是否已连接到该设备
  if (connection.isConnected.value && connection.currentDevice.value?.address === service.address) {
    return
  }

  const device: RemoteDevice = {
    id: `${service.address}:${service.port}`,
    name: service.device_name,
    address: service.address,
    port: service.port,
    isPaired: false,
  }

  // 跳转到连接页面并触发连接
  // 通过 router state 传递设备信息
  // 导航到 mobile-home（滑动容器）page 0，而非独立路由
  router.push({
    name: 'mobile-home',
    query: { page: '0' },
    state: { mdnsDevice: device } as any,
  })
}

onMounted(() => {
  startScan()
})

onUnmounted(() => {
  stopScan()
})
</script>
