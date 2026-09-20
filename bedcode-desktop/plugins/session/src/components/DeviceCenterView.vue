<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+IP:端口，右刷新/生成 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3">
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('pairing.sidebar.title') }}
        </h2>
        <span class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]"
          >{{ displayIp }}:{{ port }}</span
        >
      </div>
      <div class="flex items-center gap-2">
        <button class="wb-btn-ghost" @click="refreshAll">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"
            />
          </svg>
          {{ t('pairing.button.refresh') }}
        </button>
        <button class="wb-btn-primary" :disabled="isGenerating" @click="generateCode">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z"
            />
          </svg>
          {{ t('pairing.code.generate') }}
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
          <div
            class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-2.5 flex items-center gap-3 flex-wrap"
          >
            <span
              class="text-[calc(11px*var(--ui-scale))] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]"
            >
              {{ t('pairing.network.title') }}
            </span>
            <span class="text-[var(--border)]">|</span>
            <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{
              t('pairing.network.ipv4Address')
            }}</span>
            <!-- 固定宽度：行内条带布局，避免 Select 块级根元素撑满整行 -->
            <Select
              :model-value="selectedHost || ''"
              :options="ipOptions"
              :placeholder="t('pairing.network.notSelected')"
              size="sm"
              class="wb-mono w-[200px]"
              @update:model-value="handleIpSelect"
            />
            <span class="text-[var(--border)]">·</span>
            <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{
              t('pairing.network.websocketPort')
            }}</span>
            <span class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-primary)]">{{
              port
            }}</span>
            <span
              v-if="addresses.length === 0"
              class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] ml-auto"
            >
              {{ t('pairing.network.noIpv4') }}
            </span>
          </div>

          <!-- ==================== 配对码卡片（内容居中，旧版样式） ==================== -->
          <div
            class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-5 flex flex-col"
          >
            <div class="flex items-center justify-between gap-3 mb-4">
              <div class="min-w-0">
                <h4
                  class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]"
                >
                  {{ t('pairing.code.title') }}
                </h4>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                <button
                  v-if="pairingCode"
                  class="wb-btn-ghost !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  @click="cancelPairing"
                >
                  {{ t('pairing.button.cancel') }}
                </button>
                <button
                  v-else
                  class="wb-btn-primary !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  :disabled="isGenerating"
                  @click="generateCode"
                >
                  {{ t('pairing.code.generate') }}
                </button>
                <span
                  v-if="pairingCode"
                  class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--color-success-light)] text-[var(--color-success)]"
                >
                  <span
                    class="w-1.5 h-1.5 rounded-full bg-[var(--color-success)] animate-pulse"
                  ></span>
                  {{ remainingSeconds }}{{ t('pairing.time.seconds') }}
                </span>
              </div>
            </div>

            <div class="flex flex-col items-center text-center">
              <template v-if="pairingCode">
                <p
                  class="font-mono text-[calc(36px*var(--ui-scale))] font-bold tracking-[0.15em] text-[var(--text-primary)] select-all break-all text-center px-2 mb-4"
                >
                  {{ pairingCode.code }}
                </p>
                <p
                  class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed"
                >
                  {{ t('pairing.code.hint') }}
                </p>
              </template>
              <template v-else>
                <div
                  class="w-[168px] h-[168px] rounded-lg border border-dashed border-[var(--border-strong)] bg-[var(--bg-page)] flex flex-col items-center justify-center text-center px-3 gap-1.5 mb-4"
                >
                  <svg
                    class="w-7 h-7 text-[var(--text-tertiary)]"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                    />
                  </svg>
                  <p
                    class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] leading-tight"
                  >
                    {{ t('pairing.code.placeholder') }}
                  </p>
                </div>
                <p
                  class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed"
                >
                  {{ t('pairing.code.hint') }}
                </p>
              </template>
            </div>
          </div>

          <!-- ==================== QR 码卡片（内容居中，旧版样式） ==================== -->
          <div
            class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-5 flex flex-col"
          >
            <div class="flex items-center justify-between gap-3 mb-4">
              <div class="min-w-0">
                <h4
                  class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]"
                >
                  {{ t('pairing.qr.title') }}
                </h4>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                <button
                  v-if="hasQr"
                  class="wb-btn-ghost !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  @click="clearQr"
                >
                  {{ t('pairing.button.cancel') }}
                </button>
                <button
                  class="wb-btn-primary !h-7 !px-2.5 text-[calc(11px*var(--ui-scale))]"
                  :disabled="isQrLoading"
                  @click="generateQr(selectedHost || undefined)"
                >
                  {{ hasQr ? t('pairing.button.refresh') : t('pairing.qr.generate') }}
                </button>
                <span
                  v-if="hasQr"
                  class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--color-success-light)] text-[var(--color-success)]"
                >
                  <span
                    class="w-1.5 h-1.5 rounded-full bg-[var(--color-success)] animate-pulse"
                  ></span>
                  {{ qrRemainingSeconds }}{{ t('pairing.time.seconds') }}
                </span>
              </div>
            </div>

            <div class="flex flex-col items-center text-center">
              <template v-if="hasQr">
                <!-- 白底衬底保证二维码在暗色模式下可读 -->
                <div class="inline-block bg-white p-3 rounded-lg border border-[var(--border)] mb-4">
                  <canvas ref="qrCanvasRef" class="block"></canvas>
                </div>
                <p
                  class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed"
                >
                  {{ t('pairing.qr.hint') }}
                </p>
                <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-1.5">
                  {{ t('pairing.qr.singleUse') }}
                </p>
              </template>
              <template v-else>
                <div
                  class="w-[168px] h-[168px] rounded-lg border border-dashed border-[var(--border-strong)] bg-[var(--bg-page)] flex flex-col items-center justify-center text-center px-3 gap-1.5 mb-4"
                >
                  <svg
                    class="w-7 h-7 text-[var(--text-tertiary)]"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M3.75 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 013.75 9.375v-4.5zM3.75 14.625c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5a1.125 1.125 0 01-1.125-1.125v-4.5zM13.5 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 0113.5 9.375v-4.5z"
                    />
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M6.75 6.75h.008v.008H6.75V6.75zM6.75 16.5h.008v.008H6.75V16.5zM16.5 6.75h.008v.008H16.5V6.75zM13.5 13.5h.008v.008H13.5V13.5zM13.5 19.5h.008v.008H13.5V19.5zM19.5 13.5h.008v.008H19.5V13.5zM19.5 19.5h.008v.008H19.5V19.5zM16.5 16.5h.008v.008H16.5V16.5z"
                    />
                  </svg>
                  <p
                    class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] leading-tight"
                  >
                    {{ t('pairing.qr.placeholder') }}
                  </p>
                </div>
                <p
                  class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed"
                >
                  {{ t('pairing.qr.hint') }}
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
              {{ t('pairing.device.sectionOnline') }}
              <span class="text-[var(--text-tertiary)]">·</span> {{ onlineDevices.length }}
            </h3>
            <p
              v-if="onlineDevices.length === 0"
              class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] px-1 py-2"
            >
              {{ t('pairing.empty.noData') }}
            </p>
            <div v-else class="space-y-2">
              <article
                v-for="device in onlineDevices"
                :key="device.id"
                class="px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] hover:shadow-sm transition-shadow"
              >
                <div class="flex items-center gap-3 min-w-0">
                  <span
                    class="w-2 h-2 rounded-full shrink-0 bg-[var(--color-success)] animate-pulse"
                  ></span>
                  <p
                    class="flex-1 min-w-0 text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate"
                  >
                    {{ device.deviceName }}
                  </p>
                  <span
                    class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--color-success-light)] text-[var(--color-success)]"
                  >
                    <span class="w-1.5 h-1.5 rounded-full bg-[var(--color-success)]"></span>
                    {{ t('pairing.device.connected') }}
                  </span>
                  <span
                    class="wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]"
                    >{{ device.address }}</span
                  >
                  <button
                    class="h-7 px-2.5 rounded-[6px] border border-[var(--border)] wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                    @click="viewHistory(device.id)"
                  >
                    {{ t('pairing.device.historyView') }}
                  </button>
                  <button
                    class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
                    @click="removeDevice(device.id)"
                  >
                    {{ t('pairing.button.remove') }}
                  </button>
                </div>
                <div
                  class="mt-2 pl-5 flex items-center gap-2 flex-wrap text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]"
                >
                  <span>{{
                    t('pairing.device.pairedAt', { date: formatDate(device.pairedAt) })
                  }}</span>
                  <template v-if="device.lastSeen">
                    <span class="text-[var(--border)]">·</span>
                    <span>{{
                      t('pairing.device.lastSeen', { date: formatDate(device.lastSeen) })
                    }}</span>
                  </template>
                  <span class="text-[var(--border)]">·</span>
                  <span>{{
                    t('pairing.device.connectCount', { count: device.connectCount })
                  }}</span>
                </div>
              </article>
            </div>
          </section>

          <!-- ==================== OFFLINE 分区 ==================== -->
          <section>
            <h3 class="wb-section-title">
              {{ t('pairing.device.sectionOffline') }}
              <span class="text-[var(--text-tertiary)]">·</span> {{ offlineDevices.length }}
            </h3>
            <p
              v-if="offlineDevices.length === 0"
              class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] px-1 py-2"
            >
              {{ t('pairing.empty.noData') }}
            </p>
            <div v-else class="space-y-2">
              <article
                v-for="device in offlineDevices"
                :key="device.id"
                class="px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] hover:shadow-sm transition-shadow"
              >
                <div class="flex items-center gap-3 min-w-0">
                  <span class="w-2 h-2 rounded-full shrink-0 bg-[var(--text-tertiary)]"></span>
                  <p
                    class="flex-1 min-w-0 text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-secondary)] truncate"
                  >
                    {{ device.deviceName }}
                  </p>
                  <span
                    class="wb-mono text-[calc(11px*var(--ui-scale))] inline-flex items-center gap-1.5 px-2 h-5 rounded-[6px] bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
                  >
                    <span class="w-1.5 h-1.5 rounded-full bg-[var(--text-tertiary)]"></span>
                    {{ t('pairing.device.offline') }}
                  </span>
                  <span
                    class="wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]"
                    >{{ device.address }}</span
                  >
                  <button
                    class="h-7 px-2.5 rounded-[6px] border border-[var(--border)] wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                    @click="viewHistory(device.id)"
                  >
                    {{ t('pairing.device.historyView') }}
                  </button>
                  <button
                    class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
                    @click="removeDevice(device.id)"
                  >
                    {{ t('pairing.button.remove') }}
                  </button>
                </div>
                <div
                  class="mt-2 pl-5 flex items-center gap-2 flex-wrap text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]"
                >
                  <span>{{
                    t('pairing.device.pairedAt', { date: formatDate(device.pairedAt) })
                  }}</span>
                  <template v-if="device.lastSeen">
                    <span class="text-[var(--border)]">·</span>
                    <span>{{
                      t('pairing.device.lastSeen', { date: formatDate(device.lastSeen) })
                    }}</span>
                  </template>
                  <span class="text-[var(--border)]">·</span>
                  <span>{{
                    t('pairing.device.connectCount', { count: device.connectCount })
                  }}</span>
                </div>
              </article>
            </div>
          </section>
        </div>
      </Transition>
    </div>

    <!-- 移除设备确认 -->
    <PluginModal
      v-model="showRemoveDeviceDialog"
      :title="t('pairing.device.confirmRemove')"
      size="sm"
    >
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('pairing.device.confirmRemoveMsg') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showRemoveDeviceDialog = false">
            {{ t('pairing.button.cancel') }}
          </button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" @click="confirmRemoveDevice">
            {{ t('pairing.button.remove') }}
          </button>
        </div>
      </template>
    </PluginModal>
  </div>
</template>

<script setup lang="ts">
/**
 * DeviceCenterView — 设备与配对页面（宿主 `DevicesView.vue` 的插件版，票 14）
 *
 * What to build（票面）：设备配对 / 已配对设备两块视图搬入插件并经侧边栏目录承载，
 * 位置与外观不变；配对码展示与倒计时、QR 生成、设备撤销确认全在插件内完成。
 *
 * 与宿主原页的刻意差异（票内记录）：
 * - 取数只走插件命令通道与 PluginContext（spec D2）：宿主 `generate_pairing_code` /
 *   `list_paired_devices` 等命令面是宿主 UI 的兼容接缝，插件不得直调（C4 强校验）；
 * - 用户选择的对外 IP 落插件存储（宿主原为 `AppConfig.network.qr_host`）——QR 载荷
 *   的 host 由插件传入，线协议形状不变；
 * - 宿主 `PluginPageToolbar target="devices"` 不再渲染（宿主原页让位后挂载点消失），
 *   与票 13 会话页同一处置。
 */
import { computed, inject, onMounted, onUnmounted, ref, watch } from 'vue'
import { toast } from 'vue-sonner'
import QRCode from 'qrcode'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'
import { getRouter, type PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import PluginModal from './PluginModal.vue'
import { useDeviceCenter } from '../composables/useDeviceCenter'

const context = inject<PluginContext>('pluginContext')!
const { t } = context.i18n

const {
  pairingCode,
  remainingSeconds,
  isGenerating,
  loadNetwork,
  selectHost,
  generateCode: generatePairingCode,
  restoreCode,
  clearCode,
  qrRemainingSeconds,
  isQrLoading,
  hasQr,
  generateQr,
  restoreQr,
  clearQr,
  qrPayload,
  onlineDevices,
  offlineDevices,
  loadDevices,
  removeDevice: revokeDevice,
  port,
  addresses,
  selectedHost,
  ipOptions,
  subscribeHostEvents,
  dispose,
} = useDeviceCenter(context)

// ==================== Tab 切换 ====================
type TabKey = 'pairing' | 'devices'
const activeTab = ref<TabKey>('pairing')
const deviceTabs: { key: TabKey; label: string }[] = [
  { key: 'pairing', label: t('pairing.tab.pairing') },
  { key: 'devices', label: t('pairing.tab.devices') },
]

/** 工具栏展示的当前对外 IP（未选择时显示占位文案） */
const displayIp = computed(() => selectedHost.value || t('pairing.network.notSelected'))

// ==================== 撤销确认 ====================
const showRemoveDeviceDialog = ref(false)
const pendingDeviceId = ref<string | null>(null)

// ==================== QR 渲染 ====================

const qrCanvasRef = ref<HTMLCanvasElement | null>(null)

watch(
  () => qrPayload(),
  async (payload) => {
    if (payload && qrCanvasRef.value) {
      await QRCode.toCanvas(qrCanvasRef.value, payload, {
        width: 192,
        margin: 2,
        color: { dark: '#000000', light: '#ffffff' },
      })
    }
  },
  { flush: 'post' },
)

// ==================== 交互编排 ====================

/** 选择对外 IP（插件存储持久化；同时刷新 QR 载荷） */
function handleIpSelect(value: string | number) {
  if (value === '') return
  void selectHost(String(value)).then(() => {
    if (hasQr.value) void generateQr(String(value))
  })
}

/** 生成配对码（含失败提示；无码视为生成失败，不静默） */
async function generateCode() {
  try {
    const info = await generatePairingCode()
    if (!info?.code) {
      toast.error(t('pairing.code.generateFailedNoCode'))
    }
  } catch (e) {
    console.error('[Device Center] generate pairing code failed:', e)
    toast.error(t('pairing.code.generateFailed'))
  }
}

/** 取消配对码（清除后端状态 + 停表） */
function cancelPairing() {
  void clearCode()
}

/** 刷新设备列表（保留在线集合，仅重取配对记录） */
async function refreshAll() {
  await loadDevices()
  toast.success(t('pairing.toast.listRefreshed'))
}

/** 打开某设备的连接历史页（同一插件的另一个侧边栏目录，query 传设备 id） */
function viewHistory(deviceId: string) {
  void getRouter().push({
    name: 'plugin-sidebar-view',
    params: { pluginId: context.id, viewId: 'session.history' },
    query: { deviceId },
  })
}

function removeDevice(deviceId: string) {
  pendingDeviceId.value = deviceId
  showRemoveDeviceDialog.value = true
}

async function confirmRemoveDevice() {
  if (!pendingDeviceId.value) return
  try {
    await revokeDevice(pendingDeviceId.value)
    toast.success(t('pairing.device.removed'))
  } catch (e) {
    void e
    toast.error(t('pairing.error.revokeFailed'))
  } finally {
    showRemoveDeviceDialog.value = false
    pendingDeviceId.value = null
  }
}

// ==================== 日期格式化（跟随宿主 locale，与宿主原页同口径） ====================

const dateFormatter = computed(() => {
  const locale = context.i18n.getI18n()?.global?.locale?.value === 'en' ? 'en-US' : 'zh-CN'
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
})

function formatDate(dateStr?: string | null): string {
  if (!dateStr) return t('pairing.status.unknown')
  const date = new Date(dateStr)
  if (isNaN(date.getTime())) return t('pairing.status.unknown')
  return dateFormatter.value.format(date)
}

// ==================== 生命周期 ====================

onMounted(async () => {
  // 每一段独立捕获：任一路失败只降级本段（D7 故障隔离），不让页面整体不可用
  try {
    await Promise.all([loadNetwork(), loadDevices()])
  } catch (e) {
    console.error('[Device Center] load device data failed:', e)
    toast.error(t('pairing.error.loadFailed'))
  }
  // 恢复既有 QR / 配对码（不重新生成，与宿主原页一致）
  await restoreQr(selectedHost.value || undefined)
  await restoreCode()
  // 后端事件驱动状态流转（配对码自动清除 / 设备在线态 / 扫码消耗重生成）
  subscribeHostEvents((code) => {
    toast.info(t('pairing.code.request', { code }))
  })
})

onUnmounted(() => {
  // 停止倒计时并释放宿主事件订阅：页面切走即释放，切回重新订阅（与宿主原页同语义）
  dispose()
})
</script>

<style scoped>
/* Tab 切换过渡：淡入淡出 + 轻微 Y 位移，避免切换闪现（与宿主原页同一套过渡名） */
.tab-fade-enter-active,
.tab-fade-leave-active {
  transition:
    opacity 0.16s ease,
    transform 0.16s ease;
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
