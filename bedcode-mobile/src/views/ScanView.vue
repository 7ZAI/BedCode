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
        <h1 class="page-title">{{ t('mobile.scan.title') }}</h1>
      </div>
    </div>

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
        <div class="w-64 h-64 rounded-2xl relative" style="border: 2px solid color-mix(in srgb, var(--mobile-accent) 50%, transparent)">
          <div class="absolute -top-1 -left-1 w-8 h-8 rounded-tl-lg" style="border-top: 4px solid var(--mobile-accent); border-left: 4px solid var(--mobile-accent)"></div>
          <div class="absolute -top-1 -right-1 w-8 h-8 rounded-tr-lg" style="border-top: 4px solid var(--mobile-accent); border-right: 4px solid var(--mobile-accent)"></div>
          <div class="absolute -bottom-1 -left-1 w-8 h-8 rounded-bl-lg" style="border-bottom: 4px solid var(--mobile-accent); border-left: 4px solid var(--mobile-accent)"></div>
          <div class="absolute -bottom-1 -right-1 w-8 h-8 rounded-br-lg" style="border-bottom: 4px solid var(--mobile-accent); border-right: 4px solid var(--mobile-accent)"></div>
        </div>
      </div>

      <p v-show="!isConnecting && !errorMessage" class="absolute bottom-12 left-0 right-0 text-center text-sm" style="color: var(--mobile-text-muted)">
        {{ t('mobile.scan.scanHint') }}
      </p>

      <!-- Connecting state -->
      <div v-if="isConnecting" class="absolute inset-0 flex flex-col items-center justify-center" style="background: var(--mobile-overlay-heavy)">
        <div class="animate-spin rounded-full h-12 w-12 border-2 border-current border-t-transparent mb-4" style="color: var(--mobile-accent)"></div>
        <p class="text-lg mb-2" style="color: var(--mobile-text-secondary)">{{ connectingStep }}</p>
        <p class="text-sm" style="color: var(--mobile-text-muted)">{{ connectingDetail }}</p>
      </div>

      <!-- Error state -->
      <div v-if="errorMessage && !isConnecting" class="absolute inset-0 flex flex-col items-center justify-center" style="background: var(--mobile-overlay-heavy)">
        <svg class="w-12 h-12 mb-4" style="color: var(--mobile-chip-red)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <p class="text-lg mb-2" style="color: var(--mobile-chip-red)">{{ t('mobile.scan.connectFailed') }}</p>
        <p class="text-sm text-center px-8 mb-6" style="color: var(--mobile-text-muted)">{{ errorMessage }}</p>
        <div class="flex gap-3">
          <button
            class="px-4 py-2 rounded-xl text-sm font-medium transition-colors active:opacity-80"
            style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
            @click="goBack"
          >
            {{ t('mobile.scan.back') }}
          </button>
          <button
            class="px-4 py-2 rounded-xl text-sm font-medium transition-colors active:opacity-80"
            style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 30%, transparent)"
            @click="retry"
          >
            {{ t('mobile.scan.rescan') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Html5Qrcode } from 'html5-qrcode'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { wsAuthenticateWithQr } from '@/composables/useMobileCommands'

interface QrConnectData {
  host: string
  port: number
  token: string
}

const router = useRouter()
const connection = useMobileConnection()
const { t } = useI18n()

const readerRef = ref<HTMLElement | null>(null)
const isConnecting = ref(false)
const connectingStep = ref('')
const connectingDetail = ref('')
const errorMessage = ref('')

let html5QrCode: Html5Qrcode | null = null

function goBack() {
  router.back()
}

function retry() {
  errorMessage.value = ''
  startScanner()
}

async function handleQrScan(decodedText: string) {
  // 停止扫描
  if (html5QrCode) {
    await html5QrCode.stop()
    html5QrCode = null
  }

  // 解析 QR 数据
  let qrData: QrConnectData
  try {
    qrData = JSON.parse(decodedText)
  } catch {
    errorMessage.value = t('mobile.scan.invalidQr')
    return
  }

  if (!qrData.host || !qrData.port || !qrData.token) {
    errorMessage.value = t('mobile.scan.invalidQrData')
    return
  }

  // Step 1: WebSocket 连接
  connectingStep.value = t('mobile.scan.connecting')
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
    errorMessage.value = t('mobile.scan.cannotConnect')
    isConnecting.value = false
    connection.isConnecting.value = false
    return
  }

  // 等待连接建立完成（解决竞态问题）
  connectingStep.value = t('mobile.scan.waiting')
  const maxWaitTime = 10000 // 最多等待 10 秒
  const checkInterval = 200 // 每 200ms 检查一次
  const startTime = Date.now()

  while (!connection.isConnected.value) {
    if (Date.now() - startTime > maxWaitTime) {
      errorMessage.value = t('mobile.scan.timeout')
      isConnecting.value = false
      connection.isConnecting.value = false
      return
    }
    await new Promise(resolve => setTimeout(resolve, checkInterval))
  }

  // Step 2: 发送 QR token 认证
  connectingStep.value = t('mobile.scan.pairing')
  console.log('[Scan] QR data:', qrData)

  try {
    const creds = await wsAuthenticateWithQr(qrData.token)
    if (!creds) {
      console.error('[Scan] QR token failed')
      errorMessage.value = t('mobile.scan.qrExpired')
      isConnecting.value = false
      connection.isConnecting.value = false
      return
    }
    // 保存 JWT 凭据到 localStorage
    connection.saveCredentials(creds)
  } catch (e) {
    console.error('[Scan] QR token error:', e)
    errorMessage.value = t('mobile.scan.pairingFailed', { error: String(e) })
    isConnecting.value = false
    connection.isConnecting.value = false
    return
  }

  // 成功 - 通过 composable 保存连接历史（确保内存缓存同步）
  const address = `${qrData.host}:${qrData.port}`
  connection.addToConnectionHistory(address, 'Desktop')

  // 返回上一页
  router.back()
}

async function startScanner() {
  if (!readerRef.value) return

  try {
    html5QrCode = new Html5Qrcode('qr-reader')
    await html5QrCode.start(
      { facingMode: 'environment' },
      {
        fps: 10,
        qrbox: { width: 250, height: 250 },
      },
      handleQrScan,
      () => {} // 忽略扫描错误
    )
  } catch (e) {
    console.error('Failed to start scanner:', e)
    errorMessage.value = t('mobile.scan.cameraFailed')
  }
}

onMounted(() => {
  startScanner()
})

onUnmounted(async () => {
  if (html5QrCode) {
    await html5QrCode.stop()
  }
})
</script>