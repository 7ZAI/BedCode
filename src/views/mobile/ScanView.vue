<template>
  <div class="h-full w-full bg-black flex flex-col">
    <!-- Header -->
    <header class="bg-gray-50/90 dark:bg-dark-900/90 backdrop-blur-sm border-b border-gray-200 dark:border-dark-800 px-4 pb-3 flex items-center gap-3" style="padding-top: 12px;">
      <button
        class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-white dark:bg-dark-800 transition-colors"
        @click="goBack"
      >
        <svg class="w-5 h-5 text-gray-900 dark:text-dark-100" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h2 class="text-lg font-semibold text-gray- dark:text-dark-100">扫描二维码</h2>
    </header>

    <!-- Scanner area -->
    <div class="flex-1 relative overflow-hidden">
      <!-- Camera preview -->
      <div
        v-show="!isConnecting && !errorMessage"
        id="qr-reader"
        ref="readerRef"
        class="w-full h-full"
      ></div>

      <!-- Scan overlay frame -->
      <div
        v-show="!isConnecting && !errorMessage"
        class="absolute inset-0 pointer-events-none flex items-center justify-center"
      >
        <div class="w-64 h-64 border-2 border-primary-400 rounded-2xl relative">
          <!-- Corner accents -->
          <div class="absolute -top-1 -left-1 w-8 h-8 border-t-4 border-l-4 border-primary-400 rounded-tl-lg"></div>
          <div class="absolute -top-1 -right-1 w-8 h-8 border-t-4 border-r-4 border-primary-400 rounded-tr-lg"></div>
          <div class="absolute -bottom-1 -left-1 w-8 h-8 border-b-4 border-l-4 border-primary-400 rounded-bl-lg"></div>
          <div class="absolute -bottom-1 -right-1 w-8 h-8 border-b-4 border-r-4 border-primary-400 rounded-br-lg"></div>
        </div>
      </div>

      <p v-show="!isConnecting && !errorMessage" class="absolute bottom-12 left-0 right-0 text-center text-gray- dark:text-dark-400 text-sm">
        将二维码对准框内扫描
      </p>

      <!-- Connecting state -->
      <div v-if="isConnecting" class="absolute inset-0 flex flex-col items-center justify-center bg-black/80">
        <div class="animate-spin rounded-full h-12 w-12 border-2 border-primary-400 border-t-transparent mb-4"></div>
        <p class="text-gray- dark:text-dark-300 text-lg mb-2">{{ connectingStep }}</p>
        <p class="text-gray- dark:text-dark-500 text-sm">{{ connectingDetail }}</p>
      </div>

      <!-- Error state -->
      <div v-if="errorMessage && !isConnecting" class="absolute inset-0 flex flex-col items-center justify-center bg-black/80">
        <svg class="w-16 h-16 text-red-400 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <p class="text-red-400 text-lg mb-2">连接失败</p>
        <p class="text-gray- dark:text-dark-400 text-sm text-center px-8 mb-6">{{ errorMessage }}</p>
        <div class="flex gap-3">
          <button
            class="px-4 py-2 bg-gray-100 dark:bg-dark-700 text-gray- dark:text-dark-200 rounded-lg hover:bg-gray-200 dark:bg-dark-600 transition-colors"
            @click="goBack"
          >
            返回
          </button>
          <button
            class="px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 transition-colors"
            @click="retry"
          >
            重新扫描
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { Html5Qrcode } from 'html5-qrcode'
import { useRemoteConnection } from '@/composables/useRemoteConnection'

interface QrConnectData {
  host: string
  port: number
  token: string
}

const router = useRouter()
const connection = useRemoteConnection()

const readerRef = ref<HTMLElement | null>(null)
const isConnecting = ref(false)
const connectingStep = ref('')
const connectingDetail = ref('')
const errorMessage = ref('')

let html5QrCode: Html5Qrcode | null = null

function goBack() {
  stopScanner()
  router.push({ name: 'mobile-devices' })
}

async function startScanner() {
  if (!readerRef.value) return

  html5QrCode = new Html5Qrcode('qr-reader')

  try {
    await html5QrCode.start(
      { facingMode: 'environment' },
      {
        fps: 10,
        qrbox: { width: 250, height: 250 },
      },
      onScanSuccess,
      () => {} // ignore scan failure
    )
  } catch (err) {
    errorMessage.value = '无法打开相机，请检查相机权限设置'
  }
}

function stopScanner() {
  if (html5QrCode?.isScanning) {
    html5QrCode.stop().catch(() => {})
  }
}

async function onScanSuccess(decodedText: string) {
  stopScanner()

  // 解析 QR 数据
  let qrData: QrConnectData
  try {
    qrData = JSON.parse(decodedText)
  } catch {
    errorMessage.value = '无效的二维码格式，请重新扫描 BedCode 桌面端二维码'
    return
  }

  // 验证必要字段
  if (!qrData.host) {
    errorMessage.value = '二维码缺少主机信息，请重新扫描'
    return
  }
  if (!qrData.port) {
    errorMessage.value = '二维码缺少端口信息，请重新扫描'
    return
  }
  if (!qrData.token) {
    errorMessage.value = '二维码缺少认证信息，请重新扫描'
    return
  }

  // 开始连接流程
  isConnecting.value = true
  errorMessage.value = ''

  // Step 1: WebSocket 连接
  connectingStep.value = '正在连接...'
  connectingDetail.value = `${qrData.host}:${qrData.port}`

  try {
    await connection.connect({
      id: `qr-${Date.now()}`,
      name: 'Desktop',
      address: qrData.host,
      port: qrData.port,
      isPaired: false,
    })
  } catch {
    errorMessage.value = '无法连接到桌面端，请确保在同一网络下'
    isConnecting.value = false
    return
  }

  // Step 2: 发送 QR token 认证
  connectingStep.value = '正在配对...'
  console.log('[Scan] QR data:', qrData)

  try {
    const success = await connection.sendQrToken(qrData.token)
    if (!success) {
      console.error('[Scan] QR token failed, state:', connection.state.value)
      errorMessage.value = 'QR 码已过期或已使用，请在桌面端重新生成'
      isConnecting.value = false
      return
    }
  } catch (e) {
    console.error('[Scan] QR token error:', e)
    errorMessage.value = '配对验证失败，请重试: ' + String(e)
    isConnecting.value = false
    return
  }

  // 成功 - 返回连接页面，会自动加载会话配置
  // 不自动进入终端，让用户在连接页面选择会话配置启动
  // 保存连接历史
  const address = `${qrData.host}:${qrData.port}`
  const stored = localStorage.getItem('connection_history')
  let history: Array<{ address: string; name: string; time: number }> = []
  if (stored) {
    try {
      history = JSON.parse(stored)
    } catch {
      history = []
    }
  }
  history = history.filter(item => item.address !== address)
  history.unshift({ address, name: 'Desktop', time: Date.now() })
  if (history.length > 10) {
    history = history.slice(0, 10)
  }
  localStorage.setItem('connection_history', JSON.stringify(history))

  router.push({ name: 'mobile-home', query: { page: '0' } })
}

function retry() {
  errorMessage.value = ''
  isConnecting.value = false
  startScanner()
}

onMounted(() => {
  startScanner()
})

onUnmounted(() => {
  stopScanner()
})
</script>
