<template>
  <!-- 扫码视图：整块替换内容区（与旧连接页「扫码」分支同口径，扫码时不做 mDNS） -->
  <div v-if="showScanner" class="px-4 pb-6">
    <QrScanner @close="showScanner = false" @scan-result="onQrScanResult" />
  </div>

  <div v-else class="flex flex-col gap-3 px-4 pb-6">
    <!-- ==================== 扫码入口 ==================== -->
    <button
      type="button"
      class="w-full min-h-[44px] rounded-xl text-sm font-medium transition-colors active:opacity-80"
      :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-primary)' }"
      @click="showScanner = true"
    >
      {{ t('hub.qrConnect') }}
    </button>

    <!-- ==================== mDNS 扫描 ==================== -->
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="flex-1 min-h-[44px] rounded-xl text-sm font-medium transition-colors active:opacity-80"
        :style="{
          background: mdnsScanning ? 'var(--mobile-accent-muted)' : 'var(--mobile-accent)',
          color: mdnsScanning ? 'var(--mobile-text-primary)' : 'var(--mobile-text-on-accent)',
        }"
        @click="onToggleScan"
      >
        {{ mdnsScanning ? t('hub.stopScan') : t('hub.scan') }}
      </button>
      <input
        v-model="manualAddress"
        class="flex-[1.4] min-h-[44px] px-3 rounded-xl text-sm outline-none transition-colors"
        :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-primary)' }"
        :placeholder="t('hub.manualPlaceholder')"
        @keyup.enter="onManualConnect"
      />
      <button
        type="button"
        class="min-h-[44px] px-4 rounded-xl text-sm font-medium transition-colors active:opacity-80"
        :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
        :disabled="busy"
        @click="onManualConnect"
      >
        {{ t('hub.connect') }}
      </button>
    </div>

    <p v-if="mdnsScanning" class="text-xs" :style="{ color: 'var(--mobile-text-secondary)' }">
      {{ t('hub.scanning') }}
    </p>

    <!-- 发现设备列表 -->
    <template v-if="discovered.length > 0">
      <div class="flex flex-col gap-2">
        <button
          v-for="svc in discovered"
          :key="svc.instance_name"
          type="button"
          class="w-full rounded-xl p-3 text-left transition-colors active:opacity-90"
          :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
          :disabled="busy"
          @click="onConnectViaMdns(svc)"
        >
          <span class="block text-sm font-medium truncate" :style="{ color: 'var(--mobile-text-primary)' }">
            {{ svc.device_name || svc.instance_name }}
          </span>
          <span class="block text-xs mt-0.5 truncate" :style="{ color: 'var(--mobile-text-secondary)' }">
            {{ svc.address }}:{{ svc.port }}
          </span>
        </button>
      </div>
    </template>
    <p
      v-else-if="!mdnsScanning"
      class="rounded-xl border border-dashed px-3 py-8 text-center text-xs"
      :style="{ borderColor: 'var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
    >
      {{ t('hub.noDevices') }}<br />{{ t('hub.noDevicesHint') }}
    </p>

    <!-- ==================== 扫码结果（确认后连接 + 认证，与旧「扫描结果」卡片同口径） ==================== -->
    <div
      v-if="qrResult"
      class="rounded-xl p-3 flex flex-col gap-2"
      :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
    >
      <span class="text-sm font-medium truncate" :style="{ color: 'var(--mobile-text-primary)' }">
        {{ qrTarget }}
      </span>
      <div class="flex gap-2">
        <button
          type="button"
          class="flex-1 min-h-[40px] rounded-lg text-sm font-medium transition-colors active:opacity-80"
          :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
          :disabled="busy"
          @click="onQrConnect"
        >
          {{ t('hub.connect') }}
        </button>
        <button
          type="button"
          class="min-h-[40px] px-4 rounded-lg text-sm font-medium transition-colors active:opacity-80"
          :style="{ background: 'var(--mobile-bg-secondary)', color: 'var(--mobile-text-primary)' }"
          @click="qrResult = null"
        >
          {{ t('hub.dismiss') }}
        </button>
      </div>
    </div>

    <!-- 连接历史 -->
    <template v-if="connectionHistory.length > 0">
      <div class="flex items-center justify-between mt-2">
        <span class="text-sm font-medium" :style="{ color: 'var(--mobile-text-primary)' }">{{ t('hub.history') }}</span>
        <button type="button" class="text-xs" :style="{ color: 'var(--mobile-text-secondary)' }" @click="onClearHistory">
          {{ t('hub.clearHistory') }}
        </button>
      </div>
      <div class="flex flex-col gap-2">
        <button
          v-for="item in connectionHistory"
          :key="item.address"
          type="button"
          class="w-full rounded-xl p-3 text-left transition-colors active:opacity-90"
          :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
          :disabled="busy"
          @click="onConnectFromHistory(item)"
        >
          <span class="block text-sm font-medium truncate" :style="{ color: 'var(--mobile-text-primary)' }">{{ item.name || item.address }}</span>
          <span class="block text-xs mt-0.5 truncate" :style="{ color: 'var(--mobile-text-secondary)' }">{{ item.address }}</span>
        </button>
      </div>
    </template>

    <!-- 连接 / 配对状态区（已选目标设备时） -->
    <template v-if="isConnecting || connectingName">
      <div class="rounded-xl p-3 flex items-center gap-2" :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }">
        <span class="inline-block w-3 h-3 rounded-full animate-pulse" :style="{ background: 'var(--mobile-accent)' }" aria-hidden="true" />
        <span class="text-sm" :style="{ color: 'var(--mobile-text-primary)' }">{{ t('hub.connectingTo', { name: connectingName }) }}</span>
      </div>
    </template>

    <template v-if="deviceError">
      <p class="text-xs" :style="{ color: 'var(--mobile-warning)' }">{{ t(deviceError) }}</p>
    </template>

    <!-- 已连接/已认证：配对与生物操作 -->
    <template v-if="isAuthenticated">
      <div class="rounded-xl p-3 flex flex-col gap-2" :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }">
        <div class="flex items-center justify-between gap-2">
          <span class="text-sm font-medium truncate" :style="{ color: 'var(--mobile-text-primary)' }">
            {{ currentDevice?.name || currentDevice?.address }}
          </span>
          <span
            class="text-xs px-2 py-0.5 rounded-full shrink-0"
            :style="{ background: 'var(--mobile-accent-muted)', color: 'var(--mobile-accent)' }"
          >
            {{ t('hub.status.paired') }}
          </span>
        </div>
        <button
          type="button"
          class="min-h-[40px] rounded-lg text-sm font-medium transition-colors active:opacity-80"
          :style="{ background: 'var(--mobile-bg-secondary)', color: 'var(--mobile-text-primary)' }"
          @click="$emit('open-sessions')"
        >
          {{ t('hub.sessions') }}
        </button>
      </div>
      <div v-if="biometric && biometric.deviceSupported" class="flex flex-col gap-2">
        <button
          type="button"
          class="min-h-[40px] rounded-lg text-sm font-medium transition-colors active:opacity-80"
          :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-primary)' }"
          @click="onBiometricLogin"
        >
          {{ t('hub.biometricLogin') }}
        </button>
        <button
          v-if="!biometric.hasKey"
          type="button"
          class="text-xs" :style="{ color: 'var(--mobile-text-secondary)' }"
          @click="onBindBiometric"
        >
          {{ t('hub.pairBiometric') }}
        </button>
        <button
          v-else
          type="button"
          class="text-xs" :style="{ color: 'var(--mobile-text-secondary)' }"
          @click="onUnbindBiometric"
        >
          {{ t('hub.unpairBiometric') }}
        </button>
      </div>
    </template>

    <!-- 已连但未认证：配对入口 -->
    <template v-else-if="connectionStatus === 'connected' || connectionStatus === 'pairing'">
      <div class="rounded-xl p-3 flex flex-col gap-2" :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }">
        <p class="text-sm" :style="{ color: 'var(--mobile-text-primary)' }">{{ t('hub.pairingTitle') }}</p>
        <template v-if="pairingStage === 'idle'">
          <button
            type="button"
            class="min-h-[40px] rounded-lg text-sm font-medium transition-colors active:opacity-80"
            :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
            @click="onRequestPairing"
          >
            {{ t('hub.requestPairing') }}
          </button>
        </template>
        <template v-else>
          <p class="text-xs" :style="{ color: 'var(--mobile-text-secondary)' }">{{ t('hub.pairingHint') }}</p>
          <div class="flex gap-2">
            <input
              v-model="pairingCode"
              class="flex-1 min-h-[40px] px-3 rounded-lg text-sm outline-none"
              :style="{ background: 'var(--mobile-bg-primary)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-primary)' }"
              :placeholder="t('hub.pairingPlaceholder')"
              :disabled="pairingStage === 'verifying'"
              @keyup.enter="onVerifyPairing"
            />
            <button
              type="button"
              class="min-h-[40px] px-4 rounded-lg text-sm font-medium transition-colors active:opacity-80"
              :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
              :disabled="pairingStage === 'verifying'"
              @click="onVerifyPairing"
            >
              {{ pairingStage === 'verifying' ? t('hub.verifying') : t('hub.verify') }}
            </button>
          </div>
        </template>
        <p v-if="pairingError" class="text-xs" :style="{ color: 'var(--mobile-warning)' }">{{ t(pairingError) }}</p>
      </div>
    </template>

    <!-- 通用断开（已连接未认证 / 已认证均可） -->
    <button
      v-if="connectionStatus !== 'disconnected' && connectionStatus !== 'connecting'"
      type="button"
      class="min-h-[40px] rounded-lg text-sm font-medium transition-colors active:opacity-80"
      :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
      @click="onDisconnect"
    >
      {{ t('hub.disconnect') }}
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 设备区：mDNS 发现 + 手动连接 + 连接历史 + 配对 / 生物入口
 *
 * 数据面：mobileApi 引擎事实（mdnsServices / mdnsScanning / connectionHistory）+ 插件配对命令。
 * 派生状态（选中设备 / 配对弹层）在页面自持。
 */
import { computed, onMounted, ref } from 'vue'
import { useHostPage } from '../useHostPage'
import { parseAddress } from '../utils'
import QrScanner from './QrScanner.vue'
import type { QrPayload } from '../qr'

// 模板内 $emit('open-sessions')：声明以匹配 vue/require-explicit-emits
defineEmits<{ (e: 'open-sessions'): void }>()

const ctrl = useHostPage()
const t = ctrl.t
const logger = ctrl.logger

const manualAddress = ref('')
const pairingCode = ctrl.pairingCode

/** 扫码视图开关（与旧连接页一致：扫码时整块替换内容区，不做 mDNS） */
const showScanner = ref(false)
/** 扫码结果（确认后才连接 + 认证；token 只在内存，用完即清，不落日志） */
const qrResult = ref<QrPayload | null>(null)

const busy = computed(() => ctrl.isConnecting.value || ctrl.pairingStage.value === 'verifying')
const discovered = computed(() =>
  Array.isArray(ctrl.mdnsServices.value) ? ctrl.mdnsServices.value : [],
)
/** 扫码结果卡片上的连接目标（i18n 无关的纯地址展示） */
const qrTarget = computed(() => (qrResult.value ? `${qrResult.value.host}:${qrResult.value.port}` : ''))

const deviceError = ctrl.deviceError
const pairingError = ctrl.pairingError

// ── 模板消费面：SDK 投影的 Ref 经本组件 computed 收口 ──
// 背景：SDK 用 `import('vue').Ref` 声明投影（跨包类型），直接绑定到模板时类型解引用不生效
// （运行时 Vue 按 `__v_isRef` 解引用仍正常；此处收口是为了类型正确 + 阅读时能看清形状）。
const mdnsScanning = computed(() => ctrl.mdnsScanning.value)
const connectionHistory = computed(() => ctrl.connectionHistory.value ?? [])
const isConnecting = computed(() => ctrl.isConnecting.value)
const connectingName = computed(() => ctrl.connectingName.value)
const isAuthenticated = computed(() => ctrl.isAuthenticated.value)
const currentDevice = computed(() => ctrl.currentDevice.value)
const biometric = computed(() => ctrl.biometric.value)
const connectionStatus = computed(() => ctrl.connectionStatus.value)
const pairingStage = computed(() => ctrl.pairingStage.value)

onMounted(() => {
  void ctrl.loadHistory().catch((e) => logger.warn(`[host] load connection history failed: ${e instanceof Error ? e.message : e}`))
  void ctrl.refreshBiometric()
})

/** 扫码识别到合法载荷：关面板、留结果卡（与旧「扫描结果」卡片同口径） */
function onQrScanResult(payload: QrPayload) {
  showScanner.value = false
  qrResult.value = payload
  logger.info(`[host] qr payload accepted: target=${payload.host}:${payload.port}`)
}

/** 确认连接 + 扫码认证：连接成功才消费 token；任一步失败保留卡片供重试 */
async function onQrConnect() {
  const payload = qrResult.value
  if (!payload || busy.value) return
  try {
    await ctrl.connectDevice({ address: payload.host, port: payload.port })
  } catch {
    ctrl.toast.showToast(t(ctrl.deviceError.value || 'hub.connectFailed'), 'error')
    return
  }
  const ok = await ctrl.authenticateWithQr(payload.token)
  if (!ok) {
    ctrl.toast.showToast(t(ctrl.pairingError.value || 'hub.qrFailed'), 'error')
    return
  }
  qrResult.value = null
}

/** 统一连接入口：失败已由域层分类 + 记日志，此处只做用户可见反馈（槽位存 key 或原始文案） */
async function connectTo(target: { address: string; port: number; name?: string }) {
  if (busy.value) return
  try {
    await ctrl.connectDevice(target)
  } catch {
    ctrl.toast.showToast(t(ctrl.deviceError.value || 'hub.connectFailed'), 'error')
  }
}

async function onManualConnect() {
  const raw = manualAddress.value.trim()
  const parsed = parseAddress(raw)
  if (!parsed) {
    // 格式错误是本地校验失败（非引擎错误）：明确告知期望格式，同时落日志便于排障
    logger.warn(`[host] manual address invalid: ${raw}`)
    ctrl.toast.showToast(t('hub.manualInvalid'), 'error')
    return
  }
  await connectTo(parsed)
}

async function onConnectViaMdns(svc: any) {
  await connectTo({ address: svc.address, port: svc.port, name: svc.device_name || svc.instance_name })
}

async function onConnectFromHistory(item: any) {
  const parsed = parseAddress(`${item.address}`)
  if (!parsed) {
    logger.warn(`[host] history entry address invalid: ${item.address}`)
    return
  }
  await connectTo({ ...parsed, name: item.name })
}

async function onToggleScan() {
  if (ctrl.mdnsScanning.value) await ctrl.stopScan()
  else await ctrl.startScan(false)
}

async function onClearHistory() {
  try {
    await ctrl.clearHistory()
  } catch (e) {
    logger.error(`[host] clear connection history failed: ${e instanceof Error ? e.message : e}`)
    ctrl.toast.showToast(t('hub.historyClearFailed'), 'error')
  }
}

async function onRequestPairing() {
  try {
    await ctrl.requestPairing()
  } catch (e) {
    logger.error(`[host] request pairing failed: ${e instanceof Error ? e.message : e}`)
    ctrl.toast.showToast(t('hub.pairingFailed'), 'error')
  }
}

async function onVerifyPairing() {
  if (!pairingCode.value.trim()) return
  const ok = await ctrl.verifyPairingCode(pairingCode.value.trim())
  if (!ok) ctrl.toast.showToast(t(ctrl.pairingError.value || 'hub.pairingFailed'), 'error')
}

async function onBiometricLogin() {
  const ok = await ctrl.authenticateWithBiometric()
  if (!ok) ctrl.toast.showToast(t('hub.pairingFailed'), 'error')
}

async function onBindBiometric() {
  const ok = await ctrl.bindBiometric()
  await ctrl.refreshBiometric()
  if (!ok) ctrl.toast.showToast(t('hub.biometricBindFailed'), 'error')
}

async function onUnbindBiometric() {
  const ok = await ctrl.unbindBiometric()
  await ctrl.refreshBiometric()
  if (!ok) ctrl.toast.showToast(t('hub.biometricUnbindFailed'), 'error')
}

async function onDisconnect() {
  try {
    await ctrl.disconnect()
  } catch (e) {
    logger.error(`[host] disconnect failed: ${e instanceof Error ? e.message : e}`)
  }
}
</script>