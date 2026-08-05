<template>
  <div class="h-full w-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="flex items-center gap-3">
        <button
          class="flex-shrink-0 w-9 h-9 flex items-center justify-center rounded-lg transition-colors active:opacity-80"
          style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
          @click="goBack"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h1 class="page-title">{{ t('mobile.discover.title') }}</h1>
      </div>
    </div>

    <!-- Scanning status -->
    <div v-if="isScanning" class="px-4 pb-3 flex items-center gap-3">
      <div class="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" style="color: var(--mobile-accent)" />
      <span class="group-row-sub">{{ t('mobile.discover.scanning') }}</span>
      <span v-if="discoveredServices.length > 0" class="text-sm font-medium ml-auto" style="color: var(--mobile-accent)">
        {{ t('mobile.discover.deviceFound', { count: discoveredServices.length }) }}
      </span>
    </div>

    <!-- Device list -->
    <div class="flex-1 overflow-auto px-4 pb-4">
      <!-- Empty state -->
      <div v-if="discoveredServices.length === 0 && !isScanning" class="flex flex-col items-center justify-center h-full">
        <svg class="w-12 h-12 mb-4" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.858 15.355-5.858 21.213 0" />
        </svg>
        <p class="group-row-sub mb-1">{{ t('mobile.discover.noDevices') }}</p>
        <p class="text-sm" style="color: var(--mobile-text-disabled)">{{ t('mobile.discover.noDevicesHint') }}</p>
      </div>

      <!-- Scanning empty state (show animation) -->
      <div v-if="discoveredServices.length === 0 && isScanning" class="flex flex-col items-center justify-center h-full">
        <div class="relative mb-6">
          <div class="w-28 h-28 rounded-full relative" style="border: 2px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)">
            <div class="absolute inset-2 rounded-full" style="border: 1px solid color-mix(in srgb, var(--mobile-accent) 10%, transparent)"></div>
            <div class="absolute inset-4 rounded-full" style="border: 1px solid color-mix(in srgb, var(--mobile-accent) 5%, transparent)"></div>
            <div class="absolute inset-0 animate-[spin_3s_linear_infinite] origin-center">
              <div class="w-1/2 h-0.5 absolute top-1/2 left-1/2 -translate-y-1/2" style="background: linear-gradient(to right, var(--mobile-accent), transparent)"></div>
            </div>
            <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-3 h-3 rounded-full" style="background: var(--mobile-accent)"></div>
          </div>
        </div>
        <p class="group-row-sub">{{ t('mobile.discover.scanning') }}</p>
      </div>

      <!-- Device cards -->
      <div v-if="discoveredServices.length > 0" class="group-card">
        <div
          v-for="service in discoveredServices"
          :key="service.instance_name"
          class="group-row group-row-btn cursor-pointer"
          @click="handleConnect(service)"
        >
          <span class="icon-chip chip-cyan">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
            </svg>
          </span>
          <div class="flex-1 min-w-0">
            <div class="group-row-title">{{ service.device_name }}</div>
            <div class="group-row-sub font-mono">{{ service.address }}:{{ service.port }}</div>
          </div>
          <span class="status-badge badge-cyan">{{ service.platform }}</span>
          <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </div>
      </div>
    </div>

    <!-- Bottom actions -->
    <div class="p-4" style="padding-bottom: max(1rem, var(--safe-area-bottom, 0px))">
      <button
        v-if="isScanning"
        class="w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
        style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
        @click="stopScan"
      >
        {{ t('mobile.discover.stopScan') }}
      </button>
      <button
        v-else
        class="w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
        style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)"
        @click="startScan"
      >
        {{ t('mobile.discover.restartScan') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
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
