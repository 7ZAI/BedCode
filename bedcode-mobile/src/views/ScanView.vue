<template>
  <div class="h-full w-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="flex items-center gap-3">
        <button
          class="flex-shrink-0 w-11 h-11 flex items-center justify-center rounded-lg transition-colors active:opacity-80"
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

      <!-- 取景器遮罩 + 定位框（框外压暗，框内透明） -->
      <template v-if="!isConnecting && !errorMessage">
        <!-- 四块压暗遮罩：中间留出 256px 取景窗口 -->
        <div class="absolute inset-0 pointer-events-none">
          <div class="absolute top-0 left-0 right-0" style="height: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"></div>
          <div class="absolute bottom-0 left-0 right-0" style="height: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"></div>
          <div class="absolute left-0" style="top: calc(50% - 8rem); bottom: calc(50% - 8rem); width: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"></div>
          <div class="absolute right-0" style="top: calc(50% - 8rem); bottom: calc(50% - 8rem); width: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"></div>
        </div>

        <!-- 定位框 + 扫描线 + 提示文案（框下紧贴，消除大段留白） -->
        <div class="absolute inset-0 pointer-events-none flex flex-col items-center justify-center">
          <div class="w-64 h-64 relative">
            <div class="absolute -top-1 -left-1 w-8 h-8 rounded-tl-lg" style="border-top: 4px solid var(--mobile-accent); border-left: 4px solid var(--mobile-accent)"></div>
            <div class="absolute -top-1 -right-1 w-8 h-8 rounded-tr-lg" style="border-top: 4px solid var(--mobile-accent); border-right: 4px solid var(--mobile-accent)"></div>
            <div class="absolute -bottom-1 -left-1 w-8 h-8 rounded-bl-lg" style="border-bottom: 4px solid var(--mobile-accent); border-left: 4px solid var(--mobile-accent)"></div>
            <div class="absolute -bottom-1 -right-1 w-8 h-8 rounded-br-lg" style="border-bottom: 4px solid var(--mobile-accent); border-right: 4px solid var(--mobile-accent)"></div>
            <!-- 扫描线动画（2s 循环） -->
            <div class="scan-line"></div>
          </div>
          <p class="mt-10 text-sm" style="color: var(--mobile-text-secondary)">
            {{ t('mobile.scan.scanHint') }}
          </p>
        </div>
      </template>

      <!-- 底部工具栏：手电筒 / 相册 / 我的二维码（等宽分布，不遮挡取景器） -->
      <div
        v-if="!isConnecting && !errorMessage"
        class="absolute bottom-0 inset-x-0 pb-3 pt-10 flex justify-around pointer-events-none"
        style="background: linear-gradient(to top, rgba(0, 0, 0, 0.55), transparent)"
      >
        <button
          class="toolbar-btn pointer-events-auto"
          :class="{ active: torchOn }"
          :disabled="torchUnsupported"
          :title="t('mobile.scan.torch')"
          @click="toggleTorch"
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
          </svg>
          <span class="text-xs mt-1">{{ t('mobile.scan.torch') }}</span>
        </button>
        <button class="toolbar-btn pointer-events-auto" :title="t('mobile.scan.album')" @click="triggerAlbumPicker">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <span class="text-xs mt-1">{{ t('mobile.scan.album') }}</span>
        </button>
        <button class="toolbar-btn pointer-events-auto" :title="t('mobile.scan.myQr')" @click="showMyQrInfo = true">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
          </svg>
          <span class="text-xs mt-1">{{ t('mobile.scan.myQr') }}</span>
        </button>
      </div>

      <!-- 隐藏的相册文件选择器 -->
      <input
        ref="albumInputRef"
        type="file"
        accept="image/*"
        class="hidden"
        @change="handleAlbumFile"
      />

      <!-- Connecting state -->
      <div v-if="isConnecting" class="absolute inset-0 flex flex-col items-center justify-center" style="background: var(--mobile-overlay-heavy)">
        <div class="animate-spin rounded-full h-12 w-12 border-2 border-current border-t-transparent mb-4" style="color: var(--mobile-accent)"></div>
        <p class="text-lg mb-2" style="color: var(--mobile-text-secondary)">{{ connectingStep }}</p>
        <p class="text-sm" style="color: var(--mobile-text-muted)">{{ connectingDetail }}</p>
      </div>

      <!-- Error state（相机不可用/连接失败/相册无二维码） -->
      <div v-if="errorMessage && !isConnecting" class="absolute inset-0 flex flex-col items-center justify-center px-8" style="background: var(--mobile-overlay-heavy)">
        <svg class="w-12 h-12 mb-4" style="color: var(--mobile-chip-red)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <p class="text-lg mb-2 text-center" style="color: var(--mobile-chip-red)">{{ t('mobile.scan.connectFailed') }}</p>
        <p class="text-sm text-center mb-2" style="color: var(--mobile-text-secondary)">{{ errorMessage }}</p>
        <!-- 相机权限降级提示：引导去系统设置授权 -->
        <p v-if="isCameraError" class="text-xs text-center mb-6" style="color: var(--mobile-text-muted)">{{ t('mobile.scan.cameraPermissionHint') }}</p>
        <div class="flex gap-3">
          <button
            class="px-4 py-2.5 rounded-xl text-sm font-medium transition-colors active:opacity-80"
            style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
            @click="goBack"
          >
            {{ t('mobile.scan.back') }}
          </button>
          <button
            class="px-4 py-2.5 rounded-xl text-sm font-medium transition-colors active:opacity-80"
            style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 30%, transparent)"
            @click="retry"
          >
            {{ t('mobile.scan.rescan') }}
          </button>
        </div>
      </div>
    </div>

    <!-- 我的二维码信息弹窗 -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div v-if="showMyQrInfo" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui" @click.self="showMyQrInfo = false">
          <div class="relative w-full max-w-[clamp(280px,340px,400px)] rounded-2xl p-6 shadow-xl modal-panel" style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border)">
            <div class="flex items-center gap-3 mb-4">
              <span class="icon-chip chip-cyan">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
                </svg>
              </span>
              <h3 class="page-title text-lg">{{ t('mobile.scan.myQr') }}</h3>
            </div>
            <p class="text-sm leading-relaxed" style="color: var(--mobile-text-secondary)">{{ t('mobile.scan.myQrHint') }}</p>
            <div class="mt-4 rounded-xl p-3 flex items-center justify-between gap-3" style="background: var(--mobile-bg-primary); border: 1px solid var(--mobile-border)">
              <div class="min-w-0">
                <p class="text-xs" style="color: var(--mobile-text-muted)">{{ t('mobile.scan.myQrDeviceType') }}</p>
                <p class="text-sm font-medium mt-0.5 truncate" style="color: var(--mobile-text-primary)">{{ deviceTypeLabel }}</p>
              </div>
              <div class="text-right shrink-0">
                <p class="text-xs" style="color: var(--mobile-text-muted)">{{ t('mobile.scan.myQrPort') }}</p>
                <p class="text-sm font-medium font-mono mt-0.5" style="color: var(--mobile-text-primary)">{{ defaultPort }}</p>
              </div>
            </div>
            <button
              class="w-full mt-4 h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
              style="background: var(--mobile-accent); color: var(--mobile-text-on-accent)"
              @click="showMyQrInfo = false"
            >
              {{ t('common.button.close') }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Html5Qrcode } from 'html5-qrcode'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { usePlatform } from '@/composables/usePlatform'
import { useToast } from '@/composables/useToast'
import { wsAuthenticateWithQr } from '@/composables/useMobileCommands'

interface QrConnectData {
  host: string
  port: number
  token: string
}

const router = useRouter()
const connection = useMobileConnection()
const { settings } = useMobileSettings()
const platformInfo = usePlatform().platformInfo
const toast = useToast()
const { t } = useI18n()

const readerRef = ref<HTMLElement | null>(null)
const albumInputRef = ref<HTMLInputElement | null>(null)
const isConnecting = ref(false)
const connectingStep = ref('')
const connectingDetail = ref('')
const errorMessage = ref('')
const isCameraError = ref(false)
const showMyQrInfo = ref(false)

// 手电筒状态（部分设备不支持，启动失败时禁用按钮）
const torchOn = ref(false)
const torchUnsupported = ref(false)

/** 默认端口（来自设置，同步显示在“我的二维码”信息中） */
const defaultPort = computed(() => Number(settings.value.defaultPort) || 8765)

/** 本机平台类型（用于“我的二维码”信息展示） */
const deviceTypeLabel = computed(() => {
  const os = platformInfo.value?.osType
  return os ? `${os} · Remote Terminal` : t('mobile.scan.myQrUnknownDevice')
})

let html5QrCode: Html5Qrcode | null = null

function goBack() {
  router.back()
}

function retry() {
  errorMessage.value = ''
  startScanner()
}

async function handleQrScan(decodedText: string) {
  // 停止扫描（相册路径实例可能从未 start，需守卫 isScanning）
  if (html5QrCode?.isScanning) {
    await html5QrCode.stop()
  }
  html5QrCode = null

  // 相册扫码完成后复位 input，允许再次选择同一张图
  if (albumInputRef.value) {
    albumInputRef.value.value = ''
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

  // 已有实例在运行时先停掉（错误重试路径），避免重复 start 抛错
  if (html5QrCode?.isScanning) {
    await html5QrCode.stop()
    html5QrCode = null
  }

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
    isCameraError.value = true
    errorMessage.value = t('mobile.scan.cameraFailed')
  }
}

// ==================== 工具栏：手电筒 ====================

/** 切换手电筒：通过 applyVideoConstraints 设置 torch（摄像头不支持时禁用） */
async function toggleTorch() {
  if (!html5QrCode || torchUnsupported.value) return
  const next = !torchOn.value
  try {
    await html5QrCode.applyVideoConstraints({ advanced: [{ torch: next }] as any })
    torchOn.value = next
  } catch (e) {
    console.warn('[Scan] Torch not supported:', e)
    torchUnsupported.value = true
    toast.warning(t('mobile.scan.torchUnsupported'))
  }
}

// ==================== 工具栏：相册 ====================

/** 打开相册文件选择器 */
function triggerAlbumPicker() {
  albumInputRef.value?.click()
}

/** 从相册图片中识别二维码（复用 handleQrScan 的成功路径；
    未识别时 toast 提示且不打断相机扫描，避免误报错误态） */
async function handleAlbumFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return

  try {
    if (!html5QrCode) {
      html5QrCode = new Html5Qrcode('qr-reader')
    }
    const decoded = await html5QrCode.scanFile(file, true)
    if (decoded) {
      await handleQrScan(decoded)
    } else {
      toast.warning(t('mobile.scan.albumNoQr'))
    }
  } catch {
    // scanFile 对非二维码图片抛错，统一按“未识别”处理
    toast.warning(t('mobile.scan.albumNoQr'))
  } finally {
    input.value = ''
  }
}

onMounted(() => {
  startScanner()
})

onUnmounted(async () => {
  if (html5QrCode?.isScanning) {
    await html5QrCode.stop()
  }
})
</script>

<style scoped>
/* 扫描线动画：取景框内自上而下循环（2s） */
.scan-line {
  position: absolute;
  left: 0.75rem;
  right: 0.75rem;
  top: 0.75rem;
  height: 2px;
  border-radius: 1px;
  background: linear-gradient(90deg, transparent, var(--mobile-accent), transparent);
  box-shadow: 0 0 10px color-mix(in srgb, var(--mobile-accent) 70%, transparent);
  animation: scan-line-move 2s ease-in-out infinite;
}

@keyframes scan-line-move {
  0%, 100% { top: 0.75rem; }
  50% { top: calc(100% - 0.875rem); }
}

/* 底部工具栏按钮：44px 触控区 + 图标 + 标签。
   相机预览场景与主题无关（深浅色下都需在暗背景上可读），文字固定近白而非 theme token */
.toolbar-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.125rem;
  min-width: 4rem;
  min-height: 2.75rem;
  padding: 0.375rem 0.75rem;
  border-radius: 0.75rem;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.toolbar-btn:active {
  background: rgba(255, 255, 255, 0.15);
}

.toolbar-btn:disabled {
  opacity: 0.4;
}

.toolbar-btn.active {
  color: var(--mobile-accent);
}
</style>