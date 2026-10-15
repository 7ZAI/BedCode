<template>
  <div class="h-full w-full flex flex-col pt-2" style="background: var(--mobile-bg-primary)">
    <!-- 相机预览（扫描整合进连接页：取景器占满内容区，停止扫描由父页「二维码连接」按钮切换） -->
    <div class="relative flex-1 min-h-[52vh] rounded-xl overflow-hidden" style="background: #000">
      <div :id="readerId" ref="readerRef" class="w-full h-full"></div>

      <!-- 取景器遮罩 + 定位框（框外压暗，框内透明） -->
      <template v-if="!errorMessage">
        <!-- 四块压暗遮罩：中间留出 256px 取景窗口 -->
        <div class="absolute inset-0 pointer-events-none">
          <div
            class="absolute top-0 left-0 right-0"
            style="height: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"
          ></div>
          <div
            class="absolute bottom-0 left-0 right-0"
            style="height: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"
          ></div>
          <div
            class="absolute left-0"
            style="top: calc(50% - 8rem); bottom: calc(50% - 8rem); width: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"
          ></div>
          <div
            class="absolute right-0"
            style="top: calc(50% - 8rem); bottom: calc(50% - 8rem); width: calc(50% - 8rem); background: rgba(0, 0, 0, 0.6)"
          ></div>
        </div>

        <!-- 定位框 + 扫描线 + 提示文案（框下紧贴，消除大段留白） -->
        <div class="absolute inset-0 pointer-events-none flex flex-col items-center justify-center">
          <div class="w-64 h-64 relative">
            <div
              class="absolute -top-1 -left-1 w-8 h-8 rounded-tl-lg"
              style="border-top: 4px solid var(--mobile-accent); border-left: 4px solid var(--mobile-accent)"
            ></div>
            <div
              class="absolute -top-1 -right-1 w-8 h-8 rounded-tr-lg"
              style="border-top: 4px solid var(--mobile-accent); border-right: 4px solid var(--mobile-accent)"
            ></div>
            <div
              class="absolute -bottom-1 -left-1 w-8 h-8 rounded-bl-lg"
              style="border-bottom: 4px solid var(--mobile-accent); border-left: 4px solid var(--mobile-accent)"
            ></div>
            <div
              class="absolute -bottom-1 -right-1 w-8 h-8 rounded-br-lg"
              style="border-bottom: 4px solid var(--mobile-accent); border-right: 4px solid var(--mobile-accent)"
            ></div>
            <!-- 扫描线动画（2s 循环） -->
            <div class="scan-line"></div>
          </div>
          <p class="mt-10 text-sm" style="color: var(--mobile-text-secondary)">
            {{ t('hub.scanHint') }}
          </p>
        </div>
      </template>

      <!-- 底部工具栏：手电筒 / 相册（等宽分布，不遮挡取景器） -->
      <div
        v-if="!errorMessage"
        class="absolute bottom-0 inset-x-0 pb-3 pt-10 flex justify-around pointer-events-none"
        style="background: linear-gradient(to top, rgba(0, 0, 0, 0.55), transparent)"
      >
        <button
          class="toolbar-btn pointer-events-auto"
          :class="{ active: torchOn }"
          :disabled="torchUnsupported"
          :title="t('hub.scanTorch')"
          @click="toggleTorch"
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
            />
          </svg>
          <span class="text-xs mt-1">{{ t('hub.scanTorch') }}</span>
        </button>
        <button class="toolbar-btn pointer-events-auto" :title="t('hub.scanAlbum')" @click="triggerAlbumPicker">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
            />
          </svg>
          <span class="text-xs mt-1">{{ t('hub.scanAlbum') }}</span>
        </button>
      </div>

      <!-- 隐藏的相册文件选择器 -->
      <input ref="albumInputRef" type="file" accept="image/*" class="hidden" @change="handleAlbumFile" />

      <!-- Error state（相机不可用 / 二维码无效 / 相册无二维码） -->
      <div
        v-if="errorMessage"
        class="absolute inset-0 flex flex-col items-center justify-center px-8"
        style="background: var(--mobile-overlay-heavy)"
      >
        <svg
          class="w-12 h-12 mb-4"
          style="color: var(--mobile-chip-red)"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"
          />
        </svg>
        <p class="text-lg mb-2 text-center" style="color: var(--mobile-chip-red)">
          {{ t('hub.scanFailedTitle') }}
        </p>
        <p class="text-sm text-center mb-2" style="color: var(--mobile-text-secondary)">{{ errorMessage }}</p>
        <!-- 相机权限降级提示：引导去系统设置授权 -->
        <p v-if="isCameraError" class="text-xs text-center mb-6" style="color: var(--mobile-text-muted)">
          {{ t('hub.scanCameraPermissionHint') }}
        </p>
        <div class="flex gap-3">
          <button
            class="px-4 py-2.5 rounded-xl text-sm font-medium transition-colors active:opacity-80"
            style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
            @click="$emit('close')"
          >
            {{ t('hub.scanBack') }}
          </button>
          <button
            class="px-4 py-2.5 rounded-xl text-sm font-medium transition-colors active:opacity-80"
            style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent); border: 1px solid color-mix(in srgb, var(--mobile-accent) 30%, transparent)"
            @click="retry"
          >
            {{ t('hub.scanRescan') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 二维码扫描面板（自旧宿主 `src/components/ScanPanel.vue` 迁入插件）
 *
 * 行为对齐（与旧实现同口径）：
 * - 相机（facingMode: environment）常驻扫描；识别到**合法**载荷即停扫并 emit `scan-result`
 * - 无码 / 无效码不打断相机，只在面板内给错误态；错误原因区分「无效 / 信息不完整 / 相机不可用」
 * - 相册识别（`scanFile`）：未识别 → toast 提示且保留相机扫描
 * - 手电筒（torch）经 `applyVideoConstraints`；设备不支持 → 禁用按钮 + toast
 *
 * 与旧实现的差异（有意）：
 * - 载荷解析改走本域纯函数 `../qr`（`parseQrText`，含单测），不再就地 `JSON.parse`
 * - **未迁**旧面板的「我的二维码」信息弹窗（展示本机平台 + 默认端口；插件侧暂无平台投影），
 *   与旧 `QrScanner` 的同名遗留项一致，待宿主提供平台投影后补
 *
 * 安全：**不解码内容原文**（二维码含 token，C4 凭据零过境）——只记解析结论。
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { Html5Qrcode } from 'html5-qrcode'
import { useHostPage } from '../useHostPage'
import { parseQrText, type QrPayload } from '../qr'

const emit = defineEmits<{
  close: []
  /** 识别到有效二维码：停止扫描并把解析结果交给父页（父页展示「扫描结果」卡片并发起连接） */
  'scan-result': [data: QrPayload]
}>()

const ctrl = useHostPage()
const t = ctrl.t
const logger = ctrl.logger
const toast = ctrl.toast

/** 每个实例一个容器 id（html5-qrcode 按 id 找挂载点；同屏只应有一个扫描面板） */
let instanceSeq = 0
const readerId = `hub-qr-reader-${++instanceSeq}`

const readerRef = ref<HTMLElement | null>(null)
const albumInputRef = ref<HTMLInputElement | null>(null)
/** 错误槽位（i18n key，渲染一律 t()）；空串 = 无错误 */
const errorMessage = ref('')
const isCameraError = ref(false)

// 手电筒状态（部分设备不支持，启动失败时禁用按钮）
const torchOn = ref(false)
const torchUnsupported = ref(false)

let html5QrCode: Html5Qrcode | null = null

function retry() {
  errorMessage.value = ''
  isCameraError.value = false
  torchUnsupported.value = false
  torchOn.value = false
  void startScanner()
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

  const result = parseQrText(decodedText)
  if (!result.ok) {
    // 不落二维码原文（可能含 token）：只记结论
    logger.warn(`[host] qr payload rejected: reason=${result.reason}`)
    errorMessage.value = result.reason === 'malformed' ? 'hub.scanInvalid' : 'hub.scanInvalidData'
    return
  }

  emit('scan-result', result.payload)
}

async function startScanner() {
  if (!readerRef.value) return

  // 已有实例在运行时先停掉（错误重试路径），避免重复 start 抛错
  if (html5QrCode?.isScanning) {
    await html5QrCode.stop()
    html5QrCode = null
  }

  try {
    html5QrCode = new Html5Qrcode(readerId)
    await html5QrCode.start(
      { facingMode: 'environment' },
      {
        fps: 10,
        qrbox: { width: 250, height: 250 },
      },
      (text) => {
        void handleQrScan(text)
      },
      () => {
        // 逐帧未识别：属正常扫描过程，不记日志（热路径克制）
      },
    )
  } catch (e) {
    logger.error(`[host] qr camera start failed: ${e instanceof Error ? e.message : e}`)
    isCameraError.value = true
    errorMessage.value = 'hub.scanCameraFailed'
  }
}

// ==================== 工具栏：手电筒 ====================

/** 切换手电筒：通过 applyVideoConstraints 设置 torch（摄像头不支持时禁用） */
async function toggleTorch() {
  if (!html5QrCode || torchUnsupported.value) return
  const next = !torchOn.value
  try {
    await html5QrCode.applyVideoConstraints({ advanced: [{ torch: next }] as never })
    torchOn.value = next
  } catch (e) {
    logger.warn(`[host] qr torch unsupported: ${e instanceof Error ? e.message : e}`)
    torchUnsupported.value = true
    toast.showToast(t('hub.scanTorchUnsupported'), 'warning')
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
      html5QrCode = new Html5Qrcode(readerId)
    }
    const decoded = await html5QrCode.scanFile(file, true)
    if (decoded) {
      await handleQrScan(decoded)
    } else {
      toast.showToast(t('hub.scanAlbumNoQr'), 'warning')
    }
  } catch (e) {
    // scanFile 对非二维码图片抛错，统一按「未识别」处理
    logger.warn(`[host] qr album scan failed: ${e instanceof Error ? e.message : e}`)
    toast.showToast(t('hub.scanAlbumNoQr'), 'warning')
  } finally {
    input.value = ''
  }
}

onMounted(() => {
  void startScanner()
})

onUnmounted(() => {
  if (html5QrCode?.isScanning) {
    void html5QrCode.stop()
  }
  html5QrCode = null
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
  0%,
  100% {
    top: 0.75rem;
  }
  50% {
    top: calc(100% - 0.875rem);
  }
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
  transition:
    background-color 0.15s ease,
    color 0.15s ease;
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
