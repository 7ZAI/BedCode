<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header with safe area padding -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-4 pb-3" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold">会话配置</h1>
    </header>

    <!-- Connection Status Banner (connecting/配对中/错误时显示) -->
    <div v-if="connectionStatus === 'connecting' || connectionStatus === 'connected' || connectionStatus === 'pairing' || connectionStatus === 'error'" class="px-4 py-3 bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700">
      <div class="flex items-center gap-3">
        <!-- Connecting spinner -->
        <div v-if="connectionStatus === 'connecting'" class="w-5 h-5 border-2 border-primary-400 border-t-transparent rounded-full animate-spin" />
        <!-- Success icon (connected) -->
        <svg v-else-if="connectionStatus === 'connected'" class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
        <!-- Error icon -->
        <svg v-else-if="connectionStatus === 'error'" class="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <!-- Pairing icon -->
        <div v-else-if="connectionStatus === 'pairing'" class="w-5 h-5 bg-primary-400 rounded-full flex items-center justify-center">
          <span class="text-xs text-gray- dark:text-dark-900 font-bold">?</span>
        </div>

        <span class="text-sm" :class="{
          'text-gray- dark:text-dark-300': connectionStatus === 'connecting',
          'text-green-400': connectionStatus === 'connected',
          'text-red-400': connectionStatus === 'error',
          'text-primary-400': connectionStatus === 'pairing',
        }">
          {{ connectionStatusText }}
        </span>
      </div>
    </div>

    <!-- Connected Banner (连接后显示，已认证时用盾牌图标替换绿点) -->
    <div
      v-if="isConnected && currentDevice"
      class="mx-4 mt-4 p-3 bg-green-900/20 border border-green-800/30 rounded-lg"
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <!-- 已认证显示盾牌图标，未认证显示绿点 -->
          <svg v-if="connectionStatus === 'paired'" class="w-5 h-5 text-green-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
          </svg>
          <div v-else class="w-3 h-3 rounded-full bg-green-500 shrink-0"></div>
          <div>
            <p class="text-green-300 text-sm font-medium">{{ currentDevice.name }}</p>
            <p class="text-green-500/70 text-xs">{{ currentDevice.address }}</p>
          </div>
        </div>
        <button
          class="px-3 py-1.5 bg-gray-100 dark:bg-dark-700 text-gray- dark:text-dark-300 text-sm rounded-lg"
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
          <h3 class="text-gray- dark:text-dark-400 text-sm font-medium">会话配置</h3>
          <button
            class="p-2 rounded-lg active:bg-gray-100 dark:bg-dark-700 transition-colors"
            :class="{ 'opacity-50': isRefreshing }"
            :disabled="isRefreshing"
            @click="refreshConfigs"
            title="刷新配置"
          >
            <svg
              class="w-5 h-5 text-gray- dark:text-dark-400"
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
          <div v-for="i in 3" :key="i" class="bg-white dark:bg-dark-800 rounded-xl p-4 animate-pulse">
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <div class="h-5 w-32 bg-gray-200 dark:bg-dark-700 rounded mb-2"></div>
                <div class="flex items-center gap-2">
                  <div class="h-5 w-16 bg-gray-200 dark:bg-dark-700 rounded-full"></div>
                  <div class="h-4 w-20 bg-gray-200 dark:bg-dark-700 rounded"></div>
                </div>
                <div class="h-4 w-48 bg-gray-200 dark:bg-dark-700 rounded mt-2"></div>
                <div class="h-3 w-36 bg-gray-200 dark:bg-dark-700 rounded mt-1"></div>
              </div>
              <div class="h-8 w-16 bg-gray-200 dark:bg-dark-700 rounded-lg"></div>
            </div>
          </div>
        </div>

        <!-- Empty -->
        <div v-else-if="!isLoadingConfigs && sessionConfigs.length === 0 && hasLoadedConfigs" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-gray- dark:text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-gray- dark:text-dark-400">暂无会话配置</p>
          <p class="text-gray- dark:text-dark-500 text-sm mt-2">请在桌面端创建会话配置</p>
        </div>

        <!-- Config List -->
        <div v-else class="space-y-2">
          <div
            v-for="config in sessionConfigs"
            :key="config.id"
            class="bg-white dark:bg-dark-800 rounded-xl active:bg-gray-100 dark:bg-dark-700 transition-colors overflow-hidden"
          >
            <!-- 主卡片 -->
            <div class="p-4">
              <div class="flex items-start justify-between">
                <div class="flex-1 min-w-0" @click="goToSessions">
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
                    <span v-if="config.wsl_distro" class="text-gray- dark:text-dark-500 text-xs">{{ config.wsl_distro }}</span>
                  </div>
                  <p class="text-gray- dark:text-dark-400 text-sm mt-1 truncate">{{ config.command }}</p>
                  <p class="text-gray- dark:text-dark-500 text-xs mt-0.5 truncate">{{ config.working_dir }}</p>
                </div>
                <button
                  class="ml-3 px-4 py-2 bg-primary-600 text-white text-sm font-medium rounded-lg active:bg-primary-700 flex items-center gap-1.5 shrink-0"
                  :class="{ 'opacity-50': startingConfigId === config.id }"
                  :disabled="startingConfigId === config.id"
                  @click.stop="handleStartSession(config)"
                >
                  <div
                    v-if="startingConfigId === config.id"
                    class="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                  />
                  <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  启动
                </button>
              </div>

              <!-- 展开按钮和运行中的会话数量 -->
              <div
                v-if="getRunningSessionsByConfig(config.id).length > 0"
                class="mt-3 pt-3 border-t border-gray-100 dark:border-dark-600 flex items-center justify-between cursor-pointer"
                @click.stop="toggleConfigExpanded(config.id)"
              >
                <div class="flex items-center gap-2">
                  <div class="w-2 h-2 rounded-full bg-green-500 animate-pulse"></div>
                  <span class="text-green-400 text-sm">{{ getRunningSessionsByConfig(config.id).length }} 个运行中</span>
                </div>
                <svg
                  class="w-5 h-5 text-gray- dark:text-dark-400 transition-transform duration-200"
                  :class="{ 'rotate-180': expandedConfigId === config.id }"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </div>
            </div>

            <!-- 折叠的运行中会话列表 -->
            <transition name="slide">
              <div v-if="expandedConfigId === config.id" class="border-t border-gray-100 dark:border-dark-600">
                <div
                  v-for="session in getRunningSessionsByConfig(config.id)"
                  :key="session.id"
                  class="px-4 py-3 flex items-center justify-between active:bg-gray-50 dark:active:bg-dark-600"
                  @click="handleSessionClick(session)"
                >
                  <div class="flex items-center gap-3 min-w-0">
                    <div class="w-2 h-2 rounded-full bg-green-500 shrink-0"></div>
                    <span class="text-sm truncate">{{ session.name }}</span>
                  </div>
                  <button
                    class="shrink-0 p-1.5 text-gray- dark:text-dark-400 hover:text-red-400"
                    @click.stop="handleStopSession(session)"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              </div>
            </transition>
          </div>
        </div>
      </div>

      <!-- Connection History (when not connected) -->
      <div v-else>
        <h3 class="text-gray- dark:text-dark-400 text-sm font-medium mb-3 flex items-center justify-between">
          <span>连接历史</span>
          <button
            v-if="connectionHistory.length > 0"
            class="text-gray- dark:text-dark-500 text-xs"
            @click="clearHistory"
          >
            清除
          </button>
        </h3>

        <div v-if="connectionHistory.length === 0" class="text-center py-8">
          <p class="text-gray- dark:text-dark-500 text-sm">暂无连接历史</p>
        </div>

        <div v-else class="space-y-2">
          <div
            v-for="item in connectionHistory"
            :key="item.address"
            class="flex items-center justify-between p-3 bg-white dark:bg-dark-800 rounded-lg"
            @click="handleConnectFromHistory(item)"
          >
            <div class="flex items-center gap-3">
              <div class="w-10 h-10 rounded-full bg-gray-100 dark:bg-dark-700 flex items-center justify-center">
                <svg class="w-5 h-5 text-gray- dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </div>
              <div>
                <p class="font-medium text-gray- dark:text-dark-200">{{ item.name || item.address }}</p>
                <p class="text-gray- dark:text-dark-500 text-xs">{{ item.address }}</p>
              </div>
            </div>
            <button
              class="p-2 text-gray- dark:text-dark-500 hover:text-red-400"
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
    <div v-if="!isConnected" class="p-4 border-t border-gray-200 dark:border-dark-700 space-y-3 pb-safe">
      <!-- Scan QR Code Button -->
      <button
        class="w-full bg-gray-100 dark:bg-dark-700 text-white py-3 rounded-xl font-medium active:bg-gray-200 dark:bg-dark-600 flex items-center justify-center gap-2"
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
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
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
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
      placeholder="输入设备地址 (如: 10.186.131.120)"
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
    <Modal v-model="showStopConfirm" title="确认停止会话" size="sm">
      <p class="text-gray-600 dark:text-dark-300">
        确定要停止会话 "<span class="text-white font-medium">{{ pendingSession?.name || pendingSession?.id }}</span>" 吗？
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showStopConfirm = false">取消</Button>
          <Button variant="danger" :loading="isStopping" @click="confirmStop">停止</Button>
        </div>
      </template>
    </Modal>

    <!-- 全局遮罩 Loading（配对请求时显示） -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="showPairingLoading"
          class="fixed inset-0 z-[9999] flex items-center justify-center bg-black/60 backdrop-blur-sm"
        >
          <div class="bg-white dark:bg-dark-800 rounded-2xl p-6 shadow-xl flex flex-col items-center gap-4 min-w-[200px]">
            <div class="w-10 h-10 border-4 border-primary-400 border-t-transparent rounded-full animate-spin" />
            <p class="text-gray-700 dark:text-dark-200 text-sm font-medium">正在请求配对...</p>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useMobileConnection, type RemoteDevice } from '@/modules/mobile/composables/useMobileConnection'
import { useToast } from '@/modules/shared/composables/useToast'
import BottomSheet from '@/modules/mobile/components/BottomSheet.vue'
import PairingInput from '@/modules/mobile/components/PairingInput.vue'
import Modal from '@/modules/shared/components/Modal.vue'
import Button from '@/modules/shared/components/Button.vue'

const router = useRouter()
const connection = useMobileConnection()
const toast = useToast()

// 使用全局状态
const activeSessions = connection.activeSessions
const sessionConfigs = connection.sessionConfigs
const connectionHistory = connection.connectionHistory
const isLoadingConfigs = connection.isLoadingConfigs
const hasLoadedConfigs = connection.hasLoadedConfigs

// 根据配置ID获取运行中的会话
function getRunningSessionsByConfig(configId: string) {
  return activeSessions.value.filter(s => s.config_id === configId && (s.status === 'running' || s.status === 'waiting_input'))
}

// 展开/折叠配置
function toggleConfigExpanded(configId: string) {
  expandedConfigId.value = expandedConfigId.value === configId ? null : configId
}

// 点击会话跳转到终端
function handleSessionClick(session: any) {
  connection.activeSessionId.value = session.id
  router.push({
    name: 'mobile-terminal',
    params: { id: currentDevice.value?.id || 'default' },
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

// Session configs type (for template use)
interface SessionConfigSummary {
  id: string
  name: string
  environment: string
  wsl_distro?: string
  working_dir: string
  command: string
}

const isRefreshing = ref(false)
const startingConfigId = ref<string | null>(null)

// 展开的会话配置ID（用于显示运行中的会话）
const expandedConfigId = ref<string | null>(null)

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
      return `正在连接 ${pendingDevice.value?.name || '连接'}...`
    case 'connected':
      return '已连接，正在请求配对...'
    case 'pairing':
      return '请在桌面端查看 6 位配对码并输入'
    case 'paired':
      return '已认证，连接正常'
    case 'error':
      return connectionError.value || '连接失败'
    default:
      return connection.connectionStatus.value === 'disconnected' ? '未连接' : ''
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
      toast.success(`会话 "${config.name}" 启动成功`)

      // 跳转到会话列表页面，而不是直接进入终端
      router.push({ name: 'mobile-sessions' })
    } else {
      console.error('Failed to start session: no session_id returned')
      toast.error('启动会话失败：未返回会话ID')
    }
  } catch (e) {
    console.error('Failed to start session:', e)
    toast.error(`启动会话失败: ${e}`)
  } finally {
    startingConfigId.value = null
  }
}

// 从扫描等页面返回时重新加载连接历史
onActivated(() => {
  connection.loadConnectionHistory()
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
    // Step 1: Connect to device - 状态由后端事件驱动
    await connection.connect(device)
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
        setTimeout(() => reject(new Error('配对请求超时，请确保桌面端正在运行')), 15000)
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
      await connection.disconnect()
    } finally {
      showPairingLoading.value = false  // 隐藏全局遮罩 loading
    }
  } catch (error) {
    connectionError.value = String(error)
    console.error('[DevicesView] startConnection failed:', error)
    // 连接失败时确保状态正确
    // 后端会发送 ws_error 事件，前端状态会变为 'error'
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
  connectionError.value = '用户取消连接'
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
  connection.clearSessionConfigs()
  connection.clearActiveSessions()
}

function goToSessions() {
  router.push({ name: 'mobile-sessions' })
}
</script>

<style scoped>
.slide-enter-active,
.slide-leave-active {
  transition: all 0.2s ease;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  max-height: 0;
}

.slide-enter-to,
.slide-leave-from {
  max-height: 200px;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
