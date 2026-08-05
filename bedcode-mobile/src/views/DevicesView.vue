<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="page-header-row">
        <div>
          <h1 class="page-title">{{ t('mobile.connection.title') }}</h1>
          <p class="page-subtitle">
            <template v-if="isConnected && currentDevice">
              {{ currentDevice.name }} · {{ currentDevice.address }}
            </template>
            <template v-else>
              {{ connectionStatusText }}
            </template>
          </p>
        </div>
        <button
          class="text-[13px] font-medium pb-1 transition-colors active:opacity-80"
          style="color: var(--mobile-accent)"
          :class="{ 'opacity-50': connection.isConnecting.value }"
          :disabled="connection.isConnecting.value"
          @click="$router.push({ name: 'mobile-discover' })"
        >
          {{ t('mobile.connection.discoverDevices') }}
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 overflow-y-auto overflow-x-hidden px-4 min-h-0">
      <!-- Connection Status (connecting / error) -->
      <Transition name="fade">
      <div
        v-if="connectionStatus === 'connecting' || connectionStatus === 'error'"
        class="mb-4"
      >
        <div class="group-card">
          <div class="group-row">
            <div v-if="connectionStatus === 'connecting'" class="w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" style="color: var(--mobile-accent)" />
            <svg v-else class="w-5 h-5" style="color: var(--mobile-chip-red)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
            </svg>
            <span class="group-row-sub">{{ connectionStatusText }}</span>
          </div>
        </div>
      </div>
      </Transition>

      <!-- Connected: Session Configs -->
      <div v-if="isConnected" class="pb-8 space-y-6">
        <!-- Connected device info -->
        <section v-if="currentDevice">
          <h2 class="group-section-title">{{ t('mobile.connection.currentConnection') || '当前连接' }}</h2>
          <div class="group-card">
            <div class="group-row">
              <span class="icon-chip chip-emerald">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </span>
              <div class="flex-1 min-w-0">
                <div class="group-row-title">{{ currentDevice.name }}</div>
                <div class="group-row-sub font-mono">{{ currentDevice.address }}</div>
              </div>
              <span class="status-badge badge-emerald">
                <span class="status-dot dot-emerald"></span>
                {{ connectionStatus === 'paired' ? (t('mobile.connection.authenticated') || '已配对') : (t('mobile.connection.paired') || '已配对') }}
              </span>
            </div>
            <button class="group-row group-row-btn" @click="handleDisconnect">
              <span class="icon-chip chip-red">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
                </svg>
              </span>
              <span class="flex-1 text-left font-medium" style="font-size: 0.9375rem; color: var(--mobile-chip-red)">{{ t('mobile.connection.disconnect') }}</span>
            </button>
          </div>
        </section>

        <!-- Session Configs -->
        <section>
          <div class="flex items-center justify-between px-1 mb-2">
            <h2 class="group-section-title !mb-0">{{ t('mobile.connection.sessionConfig') }}</h2>
            <button
              class="p-1 rounded-lg transition-colors active:opacity-80"
              style="color: var(--mobile-text-muted)"
              :class="{ 'opacity-50': isRefreshing }"
              :disabled="isRefreshing"
              @click="refreshConfigs"
              :title="t('mobile.connection.refreshConfig')"
            >
              <svg
                class="w-4 h-4"
                :class="{ 'animate-spin': isRefreshing }"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
          </div>

          <!-- Loading -->
          <div v-if="isLoadingConfigs && !hasLoadedConfigs" class="space-y-2">
            <div v-for="i in 3" :key="i" class="group-card p-4 animate-pulse">
              <div class="flex items-start gap-3">
                <div class="w-9 h-9 rounded-lg" style="background: var(--mobile-chip-zinc-bg)"></div>
                <div class="flex-1">
                  <div class="h-4 w-32 rounded mb-2" style="background: var(--mobile-chip-zinc-bg)"></div>
                  <div class="h-3 w-48 rounded" style="background: var(--mobile-chip-zinc-bg)"></div>
                </div>
              </div>
            </div>
          </div>

          <!-- Empty -->
          <div v-else-if="!isLoadingConfigs && sessionConfigs.length === 0 && hasLoadedConfigs" class="text-center py-12">
            <svg class="w-12 h-12 mx-auto mb-4" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <p class="group-row-sub">{{ t('mobile.connection.noConfig') }}</p>
            <p class="text-sm mt-2" style="color: var(--mobile-text-disabled)">{{ t('mobile.connection.noConfigHint') }}</p>
          </div>

          <!-- Config List -->
          <TransitionGroup name="config-list" tag="div" class="space-y-2">
            <SessionConfigCard
              v-for="config in sessionConfigs"
              :key="config.id"
              :config="config"
              :active-sessions="activeSessions"
              :is-starting="startingConfigId === config.id"
              @start="handleStartSession"
              @navigate-to-files="handleNavigateToFiles"
              @session-click="handleSessionClick"
              @stop-session="handleStopSession"
            />
          </TransitionGroup>
        </section>
      </div>

      <!-- Not Connected: History + Actions -->
      <div v-else class="pb-8 space-y-6">
        <!-- Connection History -->
        <section>
          <div class="flex items-center justify-between px-1 mb-2">
            <h2 class="group-section-title !mb-0">{{ t('mobile.connection.connectionHistory') }}</h2>
            <button
              v-if="connectionHistory.length > 0"
              class="text-xs transition-colors active:opacity-80"
              style="color: var(--mobile-text-muted)"
              @click="clearHistory"
            >
              {{ t('mobile.connection.clearHistory') }}
            </button>
          </div>

          <div v-if="connectionHistory.length === 0" class="text-center py-8">
            <p class="text-sm" style="color: var(--mobile-text-disabled)">{{ t('mobile.connection.noHistory') }}</p>
          </div>

          <TransitionGroup v-else name="config-list" tag="div" class="group-card">
            <div
              v-for="item in connectionHistory"
              :key="item.address"
              class="group-row group-row-btn cursor-pointer"
              @click="handleConnectFromHistory(item)"
            >
              <span class="icon-chip chip-cyan">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </span>
              <div class="flex-1 min-w-0">
                <div class="group-row-title">{{ item.name || item.address }}</div>
                <div class="group-row-sub font-mono">{{ item.address }}</div>
              </div>
              <button
                class="p-1.5 rounded-lg transition-colors active:opacity-80 flex-shrink-0"
                style="color: var(--mobile-text-disabled)"
                @click.stop="removeFromHistory(item.address)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </TransitionGroup>
        </section>
      </div>
    </div>

    <!-- Action Buttons (when not connected) -->
    <div v-if="!isConnected" class="flex-shrink-0 p-4 space-y-3" style="padding-bottom: max(1rem, var(--safe-area-bottom, 0px))">
      <button
        class="w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
        style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)"
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
        @click="$router.push({ name: 'mobile-scan' })"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
        </svg>
        {{ t('mobile.connection.scanConnect') }}
      </button>
      <button
        class="w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
        style="background: var(--mobile-group-bg); color: var(--mobile-text-secondary); border: 1px solid var(--mobile-group-border)"
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
        @click="showManualConnect = true"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
        </svg>
        {{ t('mobile.connection.manualConnect') }}
      </button>
    </div>

    <!-- Manual Connect Dialog -->
    <BottomSheet
      v-model="showManualConnect"
      :title="t('mobile.connection.connectNewDevice')"
      :placeholder="t('mobile.connection.addressPlaceholder')"
      :loading="connection.isConnecting.value"
      @submit="handleConnectManual"
      @cancel="handleCancelConnection"
    />

    <!-- Auth Method Dialog -->
    <AuthMethodDialog
      v-model="showAuthDialog"
      :can-biometric="authBiometricAvailable"
      :error="authDialogError"
      :loading="authDialogLoading"
      :default-method="mobileSettings.preferredAuthMethod === 'biometric' ? 'biometric' : 'pairing'"
      @confirm="handleAuthMethod"
      @close="handleAuthDialogClose"
    />

    <!-- Pairing Dialog -->
    <PairingInput
      v-model="showPairing"
      :loading="isPairing"
      :error="pairingError"
      @submit="handlePairingSubmit"
    />

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" :title="t('mobile.connection.confirmStop')" size="sm">
      <p style="color: var(--mobile-text-disabled)">
        {{ t('mobile.connection.confirmStopMsg', { name: pendingSession?.name || pendingSession?.id }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="isStopping" @click="confirmStop">{{ t('common.button.stop') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Disconnect Confirmation Modal -->
    <Modal v-model="showDisconnectConfirm" :title="t('mobile.connection.disconnect')" size="sm">
      <p style="color: var(--mobile-text-disabled)">
        {{ t('mobile.connection.confirmDisconnectMsg') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDisconnectConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" @click="confirmDisconnect">{{ t('mobile.connection.disconnect') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- 全局遮罩 Loading -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="showPairingLoading"
          class="fixed inset-0 z-[9999] flex items-center justify-center backdrop-blur-sm mobile-ui"
          style="background: var(--mobile-overlay)"
        >
          <div class="rounded-2xl p-6 shadow-xl flex flex-col items-center gap-4 min-w-[200px]" style="background: var(--mobile-group-bg)">
            <div class="w-10 h-10 border-4 border-current border-t-transparent rounded-full animate-spin" style="color: var(--mobile-accent)" />
            <p class="text-sm font-medium" style="color: var(--mobile-text-secondary)">{{ t('mobile.connection.pairingRequest') }}</p>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Loading Overlay: 跳转终端期间显示 -->
    <transition name="mobile-loading-fade">
      <div v-if="isNavigating" class="mobile-loading-overlay">
        <div class="mobile-loading-spinner"></div>
        <p class="mobile-loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection, type RemoteDevice } from '@/composables/useMobileConnection'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { wsGetBiometricKeyStatus } from '@/composables/useMobileCommands'
import { useToast } from '@/composables/useToast'
import BottomSheet from '@/components/BottomSheet.vue'
import PairingInput from '@/components/PairingInput.vue'
import AuthMethodDialog from '@/components/AuthMethodDialog.vue'
import Modal from '@/components/Modal.vue'
import Button from '@/components/Button.vue'
import SessionConfigCard, { type SessionConfigSummary } from '@/components/SessionConfigCard.vue'

const router = useRouter()
const connection = useMobileConnection()
const { settings: mobileSettings } = useMobileSettings()
const toast = useToast()
const { t } = useI18n()

// 使用全局状态
const activeSessions = connection.activeSessions
const sessionConfigs = connection.sessionConfigs
const connectionHistory = connection.connectionHistory
const isLoadingConfigs = connection.isLoadingConfigs
const hasLoadedConfigs = connection.hasLoadedConfigs

// 点击会话跳转到终端
const isNavigating = ref(false)

function handleSessionClick(session: any) {
  if (isNavigating.value) return
  isNavigating.value = true
  connection.activeSessionId.value = session.id
  router.push({
    name: 'mobile-terminal',
    params: { id: session.id },
  })
}

// 停止会话（带确认弹窗）
const showStopConfirm = ref(false)
const pendingSession = ref<any>(null)
const isStopping = ref(false)

function handleStopSession(session: any) {
  pendingSession.value = session
  showStopConfirm.value = true
}

async function confirmStop() {
  if (!pendingSession.value) return
  isStopping.value = true
  try {
    await connection.stopSession(pendingSession.value.id)
    showStopConfirm.value = false
    pendingSession.value = null
  } catch (e) {
    console.error('[DevicesView] Failed to stop session:', e)
  } finally {
    isStopping.value = false
  }
}

const showManualConnect = ref(false)
const showPairing = ref(false)
const showPairingLoading = ref(false)  // 全局遮罩 loading（配对请求时）
const isPairing = ref(false)
const pairingError = ref('')
const connectionError = ref('')

// 认证方式选择弹窗（JWT 失效后：生物认证 / 配对码二选一，可切换）
const showAuthDialog = ref(false)
const authBiometricAvailable = ref(false)
const authDialogError = ref('')
const authDialogLoading = ref(false)

const isRefreshing = ref(false)
const startingConfigId = ref<string | null>(null)

// Current device being connected
const pendingDevice = ref<RemoteDevice | null>(null)

// 使用后端统一的连接状态
const connectionStatus = computed(() => connection.connectionStatus.value)

// 使用统一的连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前连接的设备
const currentDevice = computed(() => connection.currentDevice.value)

// 活跃会话 ID
const activeSessionId = computed(() => connection.activeSessionId.value)

const connectionStatusText = computed(() => {
  switch (connection.connectionStatus.value) {
    case 'connecting':
      return t('mobile.connection.connecting', { name: pendingDevice.value?.name || t('mobile.nav.connection') })
    case 'connected':
      return t('mobile.connection.pairing')
    case 'pairing':
      return t('mobile.connection.enterCode')
    case 'paired':
      return t('mobile.connection.authenticated')
    case 'error':
      // connectionError 可能是 i18n key（如 'mobile.connection.unreachable'）或原始错误字符串
      // t() 对未知 key 返回原字符串，因此两种情况都能正常显示
      return connectionError.value ? t(connectionError.value) : t('mobile.connection.connectFailed')
    default:
      return connection.connectionStatus.value === 'disconnected' ? t('mobile.connection.notConnected') : ''
  }
})

// 使用全局连接历史方法
function removeFromHistory(address: string) {
  connection.removeFromConnectionHistory(address)
}

function clearHistory() {
  connection.clearConnectionHistory()
}

// 刷新会话配置
async function refreshConfigs() {
  isRefreshing.value = true
  try {
    await connection.loadSessionConfigs()
  } finally {
    isRefreshing.value = false
  }
}

// Start session from config
async function handleStartSession(config: SessionConfigSummary) {
  if (!isConnected.value || startingConfigId.value) return

  startingConfigId.value = config.id
  try {
    const result = await connection.startSession(config.id, config.name)
    if (result.sessionId) {
      // 如果返回了会话信息，添加到本地列表
      if (result.session) {
        activeSessions.value.push(result.session)
      } else {
        // 如果没有返回会话信息，手动加载
        await connection.loadActiveSessions()
      }

      // 启动成功，显示 toast 提示
      toast.success(t('mobile.connection.sessionStarted', { name: config.name }))

      // 切换到滑动容器的会话页面（page 1），而非导航到独立路由
      // 导航到 mobile-sessions 会卸载 MobileSwipeContainer，导致左右滑动失效
      router.push({ name: 'mobile-home', query: { page: '1' } })
    } else {
      console.error('Failed to start session: no session_id returned')
      toast.error(t('mobile.connection.startFailedNoId'))
    }
  } catch (e) {
    console.error('Failed to start session:', e)
    toast.error(t('mobile.connection.startFailed', { error: String(e) }))
  } finally {
    startingConfigId.value = null
  }
}

// 从扫描等页面返回时重新加载连接历史（force=true，因为 ScanView 可能更新了 localStorage）
onActivated(() => {
  // 从终端返回时重置导航状态
  isNavigating.value = false
  connection.loadConnectionHistory(true)

  // 从 DiscoverView 跳转回来时，自动连接 mDNS 发现的设备
  // keep-alive 激活时 onMounted 不会重新触发，需在 onActivated 中处理
  const mdnsDevice = history.state?.mdnsDevice as RemoteDevice | undefined
  if (mdnsDevice) {
    history.replaceState({}, '')
    connection.clearSessionConfigs()
    connection.clearActiveSessions()
    startConnection(mdnsDevice, true)
  }
})

onMounted(async () => {
  connection.loadConnectionHistory()

  // 首次挂载时也检查 mDNS 设备（非 keep-alive 场景）
  const mdnsDevice = history.state?.mdnsDevice as RemoteDevice | undefined
  if (mdnsDevice) {
    history.replaceState({}, '')
    connection.clearSessionConfigs()
    connection.clearActiveSessions()
    startConnection(mdnsDevice, true)
  }
})

// 监听连接状态变化，认证完成时加载会话数据
// 注意：重连场景下，ws_paired 事件会触发 status 变为 paired，此时需要重新加载会话
watch([isConnected, connection.connectionStatus], async ([connected, status], [oldConnected, oldStatus]) => {
  // 认证完成时加载会话数据（包括首次连接和重连）
  if (connected && status === 'paired') {
    // 如果是从非 paired 状态变为 paired，或者从断开变为连接，都需要加载
    if (oldStatus !== 'paired' || !oldConnected) {
      console.log('[DevicesView] Status changed to paired, loading sessions...')
      await connection.loadSessionConfigs()
      await connection.loadActiveSessions()
    }
  }
})

// Connect from history
async function handleConnectFromHistory(item: any) {
  const [host, portStr] = item.address.split(':')
  const savedSettings = JSON.parse(localStorage.getItem('mobile-settings') || '{}')
  const defaultPort = savedSettings.defaultPort || 8765
  const port = portStr ? parseInt(portStr) : defaultPort

  const device: RemoteDevice = {
    id: `${host}:${port}`,
    name: item.name,
    address: host,
    port,
    isPaired: false,
  }

  // 清理残留的会话数据（connect() 内部会处理断开旧连接）
  connection.clearSessionConfigs()
  connection.clearActiveSessions()

  // 从历史连接，允许使用已存储的 token 跳过配对
  await startConnection(device, true)
}

// Manual address input
async function handleConnectManual(address: string) {
  const [host, portStr] = address.split(':')
  const savedSettings = JSON.parse(localStorage.getItem('mobile-settings') || '{}')
  const defaultPort = savedSettings.defaultPort || 8765
  const port = portStr ? parseInt(portStr) : defaultPort

  const device: RemoteDevice = {
    id: `${host}:${port}`,
    name: host,
    address: host,
    port,
    isPaired: false,
  }

  // 关闭手动连接弹窗，后续由 PairingInput 接管
  showManualConnect.value = false

  // 清理残留的会话数据（connect() 内部会处理断开旧连接）
  connection.clearSessionConfigs()
  connection.clearActiveSessions()

  // 手动连接，必须走配对流程
  await startConnection(device, false)
}

// Start connection flow
// @param skipPairing - 如果为 true，尝试使用已存储的 token 跳过配对流程
//                       如果为 false，必须走配对流程（手动连接场景）
async function startConnection(device: RemoteDevice, skipPairing: boolean = false) {
  pendingDevice.value = device
  connectionError.value = ''
  connection.isConnecting.value = true

  console.log('[DevicesView] startConnection: Step 1 connect...')
  console.time('startConnection')

  try {
    // Step 1: Connect to device - 带前端超时保护
    // Rust 端有 10 秒超时，前端额外设置 12 秒超时作为兜底
    const connectTimeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(t('mobile.connection.timeout'))), 12000)
    )

    await Promise.race([
      connection.connect(device),
      connectTimeout,
    ])
    console.log('[DevicesView] startConnection: Step 1 done')

    // Step 2: 如果允许跳过配对，尝试使用已存储的 JWT token
    if (skipPairing) {
      console.log('[DevicesView] startConnection: Step 2 authenticate (skipPairing=true)...')
      const authenticated = await connection.authenticate()
      console.log('[DevicesView] startConnection: Step 2 done, authenticated=', authenticated)
      if (authenticated) {
        pendingDevice.value = null
        connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
        await connection.loadSessionConfigs()
        return
      }
    } else {
      console.log('[DevicesView] startConnection: Step 2 skipped (skipPairing=false, must pair)')
    }

    // Step 2.5: JWT 认证失败（或手动连接）→ 认证方式选择弹窗。
    // 生物认证与配对码触发时机一致，弹窗内可切换；设置决定默认显示的方式。
    const keyStatus = await wsGetBiometricKeyStatus().catch(() => null)
    authBiometricAvailable.value = !!(keyStatus?.deviceSupported && keyStatus?.hasKey)
    authDialogError.value = ''
    console.log('[DevicesView] startConnection: Step 2.5 auth method selection, canBiometric=', authBiometricAvailable.value)
    showAuthDialog.value = true
  } catch (error) {
    connectionError.value = String(error)
    console.error('[DevicesView] startConnection failed:', error)

    // 显示友好的错误提示
    const errorMsg = String(error)
    if (errorMsg.includes('timeout') || errorMsg.includes('超时')) {
      toast.error(t('mobile.connection.timeoutToast'))
    } else if (errorMsg.includes('refused') || errorMsg.includes('rejected')) {
      toast.error(t('mobile.connection.refusedToast'))
    } else if (errorMsg.includes('unreachable') || errorMsg.includes('network')) {
      toast.error(t('mobile.connection.unreachableToast'))
    } else {
      toast.error(t('mobile.connection.connectFailedToast', { error: errorMsg }))
    }

    // 连接失败时确保前后端状态一致：断开后端连接 + 重置前端状态
    await connection.disconnect()
    connectionError.value = String(error)
  } finally {
    console.timeEnd('startConnection')
    connection.isConnecting.value = false
    showPairingLoading.value = false  // 确保在任何情况下都隐藏 loading
  }
}

// Cancel connection
async function handleCancelConnection() {
  await connection.cancelConnection()
  connection.isConnecting.value = false
  connectionError.value = t('mobile.connection.userCancelled')
}

// 认证方式选择弹窗：确认后执行对应认证流程
async function handleAuthMethod(method: 'biometric' | 'pairing') {
  const device = pendingDevice.value
  if (!device) return
  showAuthDialog.value = false

  if (method === 'biometric') {
    // 生物认证：弹指纹/人脸签名挑战值，成功后建立连接
    console.log('[DevicesView] Auth method: biometric')
    authDialogLoading.value = true
    try {
      const bioOk = await connection.authenticateWithBiometric()
      if (bioOk) {
        pendingDevice.value = null
        connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
        await connection.loadSessionConfigs()
        return
      }
      // 生物认证失败/取消 → 回到选择弹窗，可切换配对码
      authDialogError.value = t('mobile.connection.biometricFailed')
      showAuthDialog.value = true
    } catch (e) {
      console.error('[DevicesView] Biometric auth error:', e)
      authDialogError.value = String(e)
      showAuthDialog.value = true
    } finally {
      authDialogLoading.value = false
    }
  } else {
    // 配对码：请求配对码后进入输入弹窗
    console.log('[DevicesView] Auth method: pairing code')
    showPairingLoading.value = true
    try {
      const pairingTimeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(t('mobile.connection.pairingTimeout'))), 15000)
      )
      await Promise.race([connection.requestPairing(), pairingTimeout])
      showPairing.value = true
      connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
    } catch (pairingError) {
      // 配对失败或超时时断开连接
      console.error('[DevicesView] Pairing failed:', pairingError)
      connectionError.value = String(pairingError)
      toast.error(String(pairingError))
      await connection.disconnect()
    } finally {
      showPairingLoading.value = false
    }
  }
}

// 用户关闭认证选择弹窗 → 断开连接，保持前后端状态一致
function handleAuthDialogClose() {
  authDialogError.value = ''
  connectionError.value = t('mobile.connection.userCancelled')
  connection.disconnect()
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
      pendingDevice.value = null

      // Load session configs instead of navigating to terminal
      await connection.loadSessionConfigs()
      // Also fetch active sessions for the Sessions tab
      await connection.loadActiveSessions()
    } else {
      pairingError.value = t('mobile.connection.codeVerifyFailed')
    }
  } catch (error) {
    pairingError.value = String(error)
  } finally {
    isPairing.value = false
  }
}

const showDisconnectConfirm = ref(false)

async function handleDisconnect() {
  showDisconnectConfirm.value = true
}

async function confirmDisconnect() {
  showDisconnectConfirm.value = false
  await connection.disconnect()
  connection.clearSessionConfigs()
  connection.clearActiveSessions()
}

// 工程目录导航：优先使用 sessionId，否则使用 configId
function handleNavigateToFiles(config: SessionConfigSummary) {
  const session = activeSessions.value.find(
    (s: any) => s.config_id === config.id || s.configId === config.id
  )
  const id = session?.id || config.id
  router.push({ name: 'mobile-files', params: { id } })
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.config-list-enter-active {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.config-list-leave-active {
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.config-list-enter-from {
  opacity: 0;
  transform: translateY(8px);
}

.config-list-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.config-list-move {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
</style>
