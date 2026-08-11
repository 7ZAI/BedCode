<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+IP:端口，右刷新/生成 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3">
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.title') }}</h2>
        <span class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ displayIp }}:{{ port }}</span>
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

    <!-- ==================== Tab 切换：设备配对 / 设备列表 ==================== -->
    <div class="px-6 pt-3 flex-shrink-0">
      <div class="flex items-center gap-1 p-1 rounded-lg bg-[var(--bg-hover)]">
        <button
          v-for="tab in deviceTabs"
          :key="tab.key"
          class="h-8 flex-1 px-4 rounded-md text-[calc(12px*var(--ui-scale))] font-medium transition-colors duration-200"
          :class="
            activeTab === tab.key
              ? 'bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
          @click="activeTab = tab.key"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-5 space-y-6">
      <Transition name="tab-fade" mode="out-in">
      <!-- ==================== Tab1 设备配对 · 网络信息 + 配对码 + QR（各占一行） ==================== -->
      <div v-if="activeTab === 'pairing'" class="space-y-5">
        <!-- ==================== 网络信息条（置顶） ==================== -->
        <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-2.5 flex items-center gap-3 flex-wrap">
          <span class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">
            {{ t('desktop.device.networkTitle') }}
          </span>
          <span class="text-[var(--border)]">|</span>
          <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{ t('desktop.device.ipv4Address') }}</span>
          <!-- 固定宽度：行内条带布局，避免 Select 块级根元素撑满整行 -->
          <Select
            :model-value="selectedIp || ''"
            :options="ipOptions"
            :placeholder="t('desktop.device.notSelected')"
            size="sm"
            class="wb-mono w-[200px]"
            @update:model-value="handleIpSelect"
          />
          <span class="text-[var(--border)]">·</span>
          <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{ t('desktop.device.websocketPort') }}</span>
          <span class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-primary)]">{{ port }}</span>
          <span v-if="ipv4Addresses.length === 0" class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] ml-auto">
            {{ t('desktop.device.noIpv4') }}
          </span>
        </div>
          <!-- ==================== 配对码卡片（内容居中，旧版样式） ==================== -->
          <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-5 flex flex-col">
            <!-- 头部：标题 + 描述，右侧操作按钮 + 倒计时徽标 -->
            <div class="flex items-center justify-between gap-3 mb-4">
              <div class="min-w-0">
                <h4 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.pairingCodeTitle') }}</h4>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                <button
                  v-if="pairingCode"
                  class="wb-btn-ghost !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  @click="cancelPairing"
                >
                  {{ t('common.button.cancel') }}
                </button>
                <button
                  v-else
                  class="wb-btn-primary !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  :disabled="isLoading"
                  @click="generateCode"
                >
                  {{ t('desktop.device.generateCode') }}
                </button>
                <span v-if="pairingCode" class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--color-success-light)] text-[var(--color-success)]">
                  <span class="w-1.5 h-1.5 rounded-full bg-[var(--color-success)] animate-pulse"></span>
                  {{ remainingSeconds }}{{ t('common.time.seconds') }}
                </span>
              </div>
            </div>

            <!-- 主体：配对码居中大字展示，下方说明 -->
            <div class="flex flex-col items-center text-center">
              <template v-if="pairingCode">
                <p class="font-mono text-[calc(36px*var(--ui-scale))] font-bold tracking-[0.15em] text-[var(--text-primary)] select-all break-all text-center px-2 mb-4">
                  {{ pairingCode.code }}
                </p>
                <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed">
                  {{ t('desktop.device.pairingCodeHint') }}
                </p>
              </template>
              <template v-else>
                <div class="w-[168px] h-[168px] rounded-lg border border-dashed border-[var(--border-strong)] bg-[var(--bg-page)] flex flex-col items-center justify-center text-center px-3 gap-1.5 mb-4">
                  <svg class="w-7 h-7 text-[var(--text-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                  <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] leading-tight">{{ t('desktop.device.pairingCodePlaceholder') }}</p>
                </div>
                <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed">
                  {{ t('desktop.device.pairingCodeHint') }}
                </p>
              </template>
            </div>
          </div>
           <!-- ==================== QR 码卡片（内容居中，旧版样式） ==================== -->
          <div class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-5 flex flex-col">
            <!-- 头部：标题 + 描述，右侧操作按钮 + 倒计时徽标 -->
            <div class="flex items-center justify-between gap-3 mb-4">
              <div class="min-w-0">
                <h4 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('desktop.device.qrTitle') }}</h4>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                <button
                  v-if="qr.hasQr.value"
                  class="wb-btn-ghost !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  @click="qr.clearQr()"
                >
                  {{ t('common.button.cancel') }}
                </button>
                <button
                  class="wb-btn-primary !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  :disabled="qr.isLoading.value"
                  @click="qr.generateQr(selectedIp || undefined)"
                >
                  {{ qr.hasQr.value ? t('common.button.refresh') : t('desktop.device.generateQr') }}
                </button>
                <span v-if="qr.hasQr.value" class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--color-success-light)] text-[var(--color-success)]">
                  <span class="w-1.5 h-1.5 rounded-full bg-[var(--color-success)] animate-pulse"></span>
                  {{ qr.remainingSeconds.value }}{{ t('common.time.seconds') }}
                </span>
              </div>
            </div>

            <!-- 主体：二维码居中展示，下方说明与操作 -->
            <div class="flex flex-col items-center text-center">
              <template v-if="qr.hasQr.value">
                <!-- 白底衬底保证二维码在暗色模式下可读 -->
                <div class="inline-block bg-white p-3 rounded-lg border border-[var(--border)] mb-4">
                  <canvas ref="qrCanvasRef" class="block"></canvas>
                </div>
                <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed">
                  {{ t('desktop.device.qrHint') }}
                </p>
                <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1.5">
                  {{ t('desktop.device.qrSingleUse') }}
                </p>
              </template>
              <template v-else>
                <div class="w-[168px] h-[168px] rounded-lg border border-dashed border-[var(--border-strong)] bg-[var(--bg-page)] flex flex-col items-center justify-center text-center px-3 gap-1.5 mb-4">
                  <svg class="w-7 h-7 text-[var(--text-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3.75 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 013.75 9.375v-4.5zM3.75 14.625c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5a1.125 1.125 0 01-1.125-1.125v-4.5zM13.5 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 0113.5 9.375v-4.5z" />
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6.75 6.75h.008v.008H6.75V6.75zM6.75 16.5h.008v.008H6.75V16.5zM16.5 6.75h.008v.008H16.5V6.75zM13.5 13.5h.008v.008H13.5V13.5zM13.5 19.5h.008v.008H13.5V19.5zM19.5 13.5h.008v.008H19.5V13.5zM19.5 19.5h.008v.008H19.5V19.5zM16.5 16.5h.008v.008H16.5V16.5z" />
                  </svg>
                  <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] leading-tight">{{ t('desktop.device.qrPlaceholder') }}</p>
                </div>
                <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed">
                  {{ t('desktop.device.qrHint') }}
                </p>
              </template>
            </div>
          </div>
      </div>

      <!-- ==================== Tab2 设备列表 · 在线 / 离线 ==================== -->
      <div v-else class="space-y-6">
      <!-- ==================== ONLINE 分区 ==================== -->
      <section>
        <h3 class="wb-section-title">
          {{ t('desktop.device.sectionOnline') }} <span class="text-[var(--text-tertiary)]">·</span> {{ onlineDevices.length }}
        </h3>
        <p v-if="onlineDevices.length === 0" class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] px-1 py-2">
          {{ t('common.misc.noData') }}
        </p>
        <div v-else class="space-y-2">
          <article
            v-for="device in onlineDevices"
            :key="device.id"
            class="px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] hover:shadow-sm transition-shadow"
          >
            <div class="flex items-center gap-3 min-w-0">
              <span class="w-2 h-2 rounded-full shrink-0 bg-[var(--color-success)] animate-pulse"></span>
              <p class="flex-1 min-w-0 text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate">{{ device.deviceName }}</p>
              <span class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--color-success-light)] text-[var(--color-success)]">
                <span class="w-1.5 h-1.5 rounded-full bg-[var(--color-success)]"></span>
                {{ t('desktop.device.connected') }}
              </span>
              <span class="wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]">{{ device.address }}</span>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-[var(--border)] wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                @click="viewHistory(device.id)"
              >
                {{ t('desktop.device.historyView') }}
              </button>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
                @click="removeDevice(device.id)"
              >
                {{ t('common.button.remove') }}
              </button>
            </div>
            <div class="mt-2 pl-5 flex items-center gap-2 flex-wrap text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
              <span>{{ t('desktop.device.pairedAt', { date: formatDate(device.pairedAt) }) }}</span>
              <template v-if="device.lastSeen">
                <span class="text-[var(--border)]">·</span>
                <span>{{ t('desktop.device.lastSeen', { date: formatDate(device.lastSeen) }) }}</span>
              </template>
              <span class="text-[var(--border)]">·</span>
              <span>{{ t('desktop.device.connectCount', { count: device.connectCount }) }}</span>
            </div>
          </article>
        </div>
      </section>

      <!-- ==================== OFFLINE 分区 ==================== -->
      <section>
        <h3 class="wb-section-title">
          {{ t('desktop.device.sectionOffline') }} <span class="text-[var(--text-tertiary)]">·</span> {{ offlineDevices.length }}
        </h3>
        <p v-if="offlineDevices.length === 0" class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] px-1 py-2">
          {{ t('common.misc.noData') }}
        </p>
        <div v-else class="space-y-2">
          <article
            v-for="device in offlineDevices"
            :key="device.id"
            class="px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] hover:shadow-sm transition-shadow"
          >
            <div class="flex items-center gap-3 min-w-0">
              <span class="w-2 h-2 rounded-full shrink-0 bg-[var(--text-tertiary)]"></span>
              <p class="flex-1 min-w-0 text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-secondary)] truncate">{{ device.deviceName }}</p>
              <span class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--bg-hover)] text-[var(--text-tertiary)]">
                <span class="w-1.5 h-1.5 rounded-full bg-[var(--text-tertiary)]"></span>
                {{ t('desktop.device.offline') }}
              </span>
              <span class="wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]">{{ device.address }}</span>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-[var(--border)] wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                @click="viewHistory(device.id)"
              >
                {{ t('desktop.device.historyView') }}
              </button>
              <button
                class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
                @click="removeDevice(device.id)"
              >
                {{ t('common.button.remove') }}
              </button>
            </div>
            <div class="mt-2 pl-5 flex items-center gap-2 flex-wrap text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
              <span>{{ t('desktop.device.pairedAt', { date: formatDate(device.pairedAt) }) }}</span>
              <template v-if="device.lastSeen">
                <span class="text-[var(--border)]">·</span>
                <span>{{ t('desktop.device.lastSeen', { date: formatDate(device.lastSeen) }) }}</span>
              </template>
              <span class="text-[var(--border)]">·</span>
              <span>{{ t('desktop.device.connectCount', { count: device.connectCount }) }}</span>
            </div>
          </article>
        </div>
      </section>
      </div>
      </Transition>
    </div>

    <!-- 移除设备确认 -->
    <Modal v-model="showRemoveDeviceDialog" :title="t('desktop.device.confirmRemove')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">{{ t('desktop.device.confirmRemoveMsg') }}</p>
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
 * Warm Workbench 风格：PAIRING（QR/配对码双卡 + 网络条）+ ONLINE/OFFLINE 设备卡
 */
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import i18n from '@/locales'
import { useDeviceStore } from '@/stores/device'
import { useSettingsStore } from '@/stores/settings'
import { usePairing, useNetwork, useConnectedDevices, type DeviceConnectionInfo, type PairingCodeInfo } from '@/composables/useTauri'
import type { PairedDevice } from '@/stores/device'
import { useQrCode } from '@/composables/useQrCode'
import { listen } from '@tauri-apps/api/event'
import Modal from '@/components/Modal.vue'
import { Select } from '@/components'
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

// ==================== Tab 切换：设备配对 / 设备列表 ====================
type TabKey = 'pairing' | 'devices'
const activeTab = ref<TabKey>('pairing')
const deviceTabs: { key: TabKey; label: string }[] = [
  { key: 'pairing', label: t('desktop.device.sectionPairing') },
  { key: 'devices', label: t('desktop.device.tabDevices') },
]

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
        width: 192,
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

// IP 下拉选项：IPv4 地址列表（value/label 同值）
const ipOptions = computed(() =>
  ipv4Addresses.value.map(ip => ({ value: ip, label: ip })),
)

/** 下拉值变化处理：占位清除行会发出空串，与原禁用占位 option 语义一致，空串直接忽略 */
function handleIpSelect(value: string | number) {
  if (value === '') return
  onIpSelect(String(value))
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
  // 注意：不在此重载全局设置——loadSettings 会整体覆盖 store（含用户刚切换的
  // theme_palette/theme），导致切页时色板回退。设置已在 main.ts 启动时加载，
  // qr_host 等网络字段直接读 store 即可（无 qr_host 时下方自动补写）
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
    console.log('[DevicesView] restored active QR token')
  }

  // 检查是否有活跃的配对码，若有则自动恢复显示
  const hasActiveCode = await pairing.checkCurrentCode()
  if (hasActiveCode && pairing.pairingCode.value) {
    console.log('[DevicesView] restoring active pairing code:', pairing.pairingCode.value)
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
      console.log('[DevicesView] device connected, clearing pairing code')
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
    console.log('[DevicesView] QR token consumed, regenerating')
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
      console.log('[DevicesView] received pairing-code-generated event:', event.payload)
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
    console.error('[DevicesView] generate pairing code failed:', e)
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

/** 跟随当前 i18n locale 格式化日期 */
const dateFormatter = computed(() => {
  const locale = i18n.global.locale.value === 'en' ? 'en-US' : 'zh-CN'
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
})

function formatDate(dateStr: string): string {
  if (!dateStr) {
    return t('common.status.unknown')
  }
  const date = new Date(dateStr)
  if (isNaN(date.getTime())) {
    return t('common.status.unknown')
  }
  return dateFormatter.value.format(date)
}
</script>

<style scoped>
/* Tab 切换过渡：淡入淡出 + 轻微 Y 位移，避免切换闪现 */
.tab-fade-enter-active,
.tab-fade-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.tab-fade-enter-from {
  opacity: 0;
  transform: translateY(4px);
}
.tab-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
