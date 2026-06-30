<template>
  <div class="h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- Header -->
    <header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)] px-4 pb-3 pt-3 flex items-center justify-between">
      <h1 class="text-lg font-semibold text-[var(--mobile-text-primary)] tracking-wide">{{ t('mobile.connection.title') }}</h1>
    </header>

    <!-- Connection Status Banner -->
    <div
      v-if="connectionStatus === 'connecting' || connectionStatus === 'connected' || connectionStatus === 'pairing' || connectionStatus === 'error'"
      class="px-4 py-3 bg-[var(--mobile-bg-secondary)] border-b border-[var(--mobile-border)]"
    >
      <div class="flex items-center gap-3">
        <!-- Connecting spinner -->
        <div v-if="connectionStatus === 'connecting'" class="w-5 h-5 border-2 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin" />
        <!-- Success icon (connected) -->
        <svg v-else-if="connectionStatus === 'connected'" class="w-5 h-5 text-[var(--mobile-success)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
        <!-- Error icon -->
        <svg v-else-if="connectionStatus === 'error'" class="w-5 h-5 text-[var(--mobile-error)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <!-- Pairing icon -->
        <div v-else-if="connectionStatus === 'pairing'" class="w-5 h-5 bg-[var(--mobile-accent-secondary)] rounded-full flex items-center justify-center">
          <span class="text-xs text-[var(--mobile-accent)] font-bold">?</span>
        </div>

        <span class="text-sm" :class="{
          'text-[var(--mobile-text-muted)]': connectionStatus === 'connecting',
          'text-[var(--mobile-success)]': connectionStatus === 'connected',
          'text-[var(--mobile-error)]': connectionStatus === 'error',
          'text-[var(--mobile-accent)]': connectionStatus === 'pairing',
        }">
          {{ connectionStatusText }}
        </span>
      </div>
    </div>

    <!-- Connected Banner -->
    <div
      v-if="isConnected && currentDevice"
      class="mx-4 mt-4 p-3 bg-[var(--mobile-success-connected-bg)] border border-[var(--mobile-success-connected-border)] rounded-xl backdrop-blur-sm shadow-[var(--mobile-card-shadow-connected)]"
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <!-- 已认证显示盾牌图标，未认证显示绿点 -->
          <svg v-if="connectionStatus === 'paired'" class="w-5 h-5 text-[var(--mobile-success)] shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
          </svg>
          <div v-else class="w-3 h-3 rounded-full bg-[var(--mobile-success)] shrink-0 shadow-[0_0_8px_rgba(16,185,129,0.5)]"></div>
          <div>
            <p class="text-[var(--mobile-success)] text-sm font-medium">{{ currentDevice.name }}</p>
            <p class="text-[var(--mobile-success)]/70 text-xs">{{ currentDevice.address }}</p>
          </div>
        </div>
        <button
          class="px-3 py-1.5 bg-[var(--mobile-error-muted)] border text-[var(--mobile-error)] text-sm rounded-lg hover:bg-[var(--mobile-error)]/20 transition-colors"
          style="border-color: color-mix(in srgb, var(--mobile-error) 40%, transparent)"
          @click="handleDisconnect"
        >
          {{ t('mobile.connection.disconnect') }}
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 overflow-auto p-4">
      <!-- Session Configs (when connected) -->
      <div v-if="isConnected">
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium tracking-wider uppercase">{{ t('mobile.connection.sessionConfig') }}</h3>
          <button
            class="p-2 rounded-lg hover:bg-[var(--mobile-accent-muted)] transition-colors"
            :class="{ 'opacity-50': isRefreshing }"
            :disabled="isRefreshing"
            @click="refreshConfigs"
            :title="t('mobile.connection.refreshConfig')"
          >
            <svg
              class="w-5 h-5 text-[var(--mobile-accent)]"
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
          <div v-for="i in 3" :key="i" class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl p-4 animate-pulse">
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <div class="h-5 w-32 bg-[var(--mobile-accent-muted)] rounded mb-2"></div>
                <div class="flex items-center gap-2">
                  <div class="h-5 w-16 bg-[var(--mobile-accent-muted)] rounded-full"></div>
                  <div class="h-4 w-20 bg-[var(--mobile-accent-muted)] rounded"></div>
                </div>
                <div class="h-4 w-48 bg-[var(--mobile-accent-muted)] rounded mt-2"></div>
                <div class="h-3 w-36 bg-[var(--mobile-accent-muted)] rounded mt-1"></div>
              </div>
              <div class="h-8 w-16 bg-[var(--mobile-accent-muted)] rounded-lg"></div>
            </div>
          </div>
        </div>

        <!-- Empty -->
        <div v-else-if="!isLoadingConfigs && sessionConfigs.length === 0 && hasLoadedConfigs" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-[var(--mobile-accent)]/30 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-[var(--mobile-text-muted)]">{{ t('mobile.connection.noConfig') }}</p>
          <p class="text-[var(--mobile-text-disabled)] text-sm mt-2">{{ t('mobile.connection.noConfigHint') }}</p>
        </div>

        <!-- Config List -->
        <div v-else class="space-y-2">
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
        </div>
      </div>

      <!-- Connection History (when not connected) -->
      <div v-else>
        <!-- Connection History Section -->
        <h3 class="text-[var(--mobile-accent)]/80 text-sm font-medium mb-3 flex items-center justify-between tracking-wider uppercase">
          <span>{{ t('mobile.connection.connectionHistory') }}</span>
          <button
            v-if="connectionHistory.length > 0"
            class="text-[var(--mobile-text-muted)] text-xs hover:text-[var(--mobile-accent)] transition-colors"
            @click="clearHistory"
          >
            {{ t('mobile.connection.clearHistory') }}
          </button>
        </h3>

        <div v-if="connectionHistory.length === 0" class="text-center py-8">
          <p class="text-[var(--mobile-text-disabled)] text-sm">{{ t('mobile.connection.noHistory') }}</p>
        </div>

        <div v-else class="space-y-2">
          <div
            v-for="item in connectionHistory"
            :key="item.address"
            class="flex items-center justify-between p-3 bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl shadow-[var(--mobile-card-shadow)] hover:border-[var(--mobile-border-active)] hover:shadow-[var(--mobile-card-shadow-hover)] transition-all cursor-pointer"
            @click="handleConnectFromHistory(item)"
          >
            <div class="flex items-center gap-3">
              <div class="w-10 h-10 rounded-full bg-[var(--mobile-accent-muted)] border border-[var(--mobile-border-hover)] flex items-center justify-center">
                <svg class="w-5 h-5 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </div>
              <div>
                <p class="font-medium text-[var(--mobile-text-secondary)]">{{ item.name || item.address }}</p>
                <p class="text-[var(--mobile-text-disabled)] text-xs">{{ item.address }}</p>
              </div>
            </div>
            <button
              class="p-2 text-[var(--mobile-text-disabled)] hover:text-[var(--mobile-error)] transition-colors"
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
    <div v-if="!isConnected" class="p-4 border-t border-[var(--mobile-border)] space-y-3 pb-safe">
      <!-- Scan QR Code Button -->
      <button
        class="w-full bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border-hover)] text-[var(--mobile-accent)] py-3 rounded-xl font-medium hover:bg-[var(--mobile-accent-muted)] transition-all flex items-center justify-center gap-2"
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
        @click="$router.push({ name: 'mobile-scan' })"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
        </svg>
        {{ t('mobile.connection.scanConnect') }}
      </button>

      <!-- Manual Connect Button -->
      <button
        class="w-full bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] py-3 rounded-xl font-medium hover:bg-[var(--mobile-accent)]/30 transition-all flex items-center justify-center gap-2"
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

    <!-- Pairing Dialog -->
    <PairingInput
      v-model="showPairing"
      :loading="isPairing"
      :error="pairingError"
      @submit="handlePairingSubmit"
    />

    <!-- Stop Confirmation Modal -->
    <Modal v-model="showStopConfirm" :title="t('mobile.connection.confirmStop')" size="sm">
      <p class="text-[var(--mobile-text-disabled)]">
        {{ t('mobile.connection.confirmStopMsg', { name: pendingSession?.name || pendingSession?.id }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" :loading="isStopping" @click="confirmStop">{{ t('common.button.stop') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- 全局遮罩 Loading（配对请求时显示） -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="showPairingLoading"
          class="fixed inset-0 z-[9999] flex items-center justify-center bg-[var(--mobile-overlay)] backdrop-blur-sm mobile-ui"
        >
          <div class="bg-[var(--mobile-bg-card)] rounded-2xl p-6 shadow-xl flex flex-col items-center gap-4 min-w-[200px]">
            <div class="w-10 h-10 border-4 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin" />
            <p class="text-[var(--mobile-text-secondary)] text-sm font-medium">{{ t('mobile.connection.pairingRequest') }}</p>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Loading Overlay: 跳转终端期间显示 -->
    <transition name="loading-fade">
      <div v-if="isNavigating" class="loading-overlay">
        <div class="loading-spinner"></div>
        <p class="loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection, type RemoteDevice } from '@/modules/mobile/composables/useMobileConnection'
import { useToast } from '@/modules/shared/composables/useToast'
import BottomSheet from '@/modules/mobile/components/BottomSheet.vue'
import PairingInput from '@/modules/mobile/components/PairingInput.vue'
import Modal from '@/modules/shared/components/Modal.vue'
import Button from '@/modules/shared/components/Button.vue'
import SessionConfigCard, { type SessionConfigSummary } from '@/modules/mobile/components/SessionConfigCard.vue'

const router = useRouter()
const connection = useMobileConnection()
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
      return connectionError.value || t('mobile.connection.connectFailed')
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

      // 跳转到会话列表页面，而不是直接进入终端
      router.push({ name: 'mobile-sessions' })
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
})

onMounted(async () => {
  connection.loadConnectionHistory()
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
  const port = portStr ? parseInt(portStr) : 8765

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
  const port = portStr ? parseInt(portStr) : 8765

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

    // Step 3: Need to pair - 带超时保护，避免卡住
    console.log('[DevicesView] startConnection: Step 3 requestPairing...')
    showPairingLoading.value = true  // 显示全局遮罩 loading
    try {
      const pairingTimeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(t('mobile.connection.pairingTimeout'))), 15000)
      )
      await Promise.race([
        connection.requestPairing(),
        pairingTimeout,
      ])
      console.log('[DevicesView] startConnection: Step 3 done, showPairing=true')
      showPairing.value = true
      connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
    } catch (pairingError) {
      // 配对失败或超时时断开连接
      console.error('[DevicesView] Pairing failed:', pairingError)
      connectionError.value = String(pairingError)
      toast.error(String(pairingError))
      await connection.disconnect()
    } finally {
      showPairingLoading.value = false  // 隐藏全局遮罩 loading
    }
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

async function handleDisconnect() {
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

/* Loading Overlay */
.loading-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: var(--mobile-bg-primary);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1rem;
}

.loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--mobile-border);
  border-top-color: var(--mobile-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.loading-text {
  font-size: 0.875rem;
  color: var(--mobile-text-muted);
  margin: 0;
}

/* Loading fade transition */
.loading-fade-enter-active,
.loading-fade-leave-active {
  transition: opacity 0.3s ease;
}

.loading-fade-enter-from,
.loading-fade-leave-to {
  opacity: 0;
}
</style>
