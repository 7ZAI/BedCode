<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- 取景器：相机预览占满内容区（停止/返回由父页按钮切换，与旧 ScanPanel 同布局） -->
    <div class="relative flex-1 min-h-[52vh] rounded-xl overflow-hidden" style="background: #000">
      <div :id="readerId" ref="readerRef" class="w-full h-full" />

      <template v-if="!errorMessage">
        <!-- 定位框遮罩 + 扫描线 -->
        <div class="absolute inset-0 pointer-events-none flex flex-col items-center justify-center">
          <div class="w-64 h-64 relative">
            <div
              class="absolute -top-1 -left-1 w-8 h-8 rounded-tl-lg"
              :style="{ borderTop: '4px solid var(--mobile-accent)', borderLeft: '4px solid var(--mobile-accent)' }"
            />
            <div
              class="absolute -top-1 -right-1 w-8 h-8 rounded-tr-lg"
              :style="{ borderTop: '4px solid var(--mobile-accent)', borderRight: '4px solid var(--mobile-accent)' }"
            />
            <div
              class="absolute -bottom-1 -left-1 w-8 h-8 rounded-bl-lg"
              :style="{ borderBottom: '4px solid var(--mobile-accent)', borderLeft: '4px solid var(--mobile-accent)' }"
            />
            <div
              class="absolute -bottom-1 -right-1 w-8 h-8 rounded-br-lg"
              :style="{ borderBottom: '4px solid var(--mobile-accent)', borderRight: '4px solid var(--mobile-accent)' }"
            />
            <div class="hub-scan-line" />
          </div>
          <p class="mt-10 text-sm text-center px-6" :style="{ color: 'var(--mobile-text-secondary)' }">
            {{ t('hub.qrScanHint') }}
          </p>
        </div>

        <!-- 底部工具栏：照明 / 相册 -->
        <div class="absolute bottom-0 inset-x-0 pb-3 pt-10 flex justify-around">
          <button
            type="button"
            class="flex flex-col items-center justify-center gap-0.5 min-w-[4rem] min-h-[2.75rem] px-3 py-1.5 rounded-xl text-xs"
            :style="{ color: torchOn ? 'var(--mobile-accent)' : 'rgba(255,255,255,0.85)' }"
            :disabled="torchUnsupported"
            :class="{ 'opacity-40': torchUnsupported }"
            @click="toggleTorch"
          >
            <svg width="22" height="22" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
            </svg>
            {{ t('hub.qrTorch') }}
          </button>
          <button
            type="button"
            class="flex flex-col items-center justify-center gap-0.5 min-w-[4rem] min-h-[2.75rem] px-3 py-1.5 rounded-xl text-xs"
            style="color: rgba(255, 255, 255, 0.85)"
            @click="pickFromAlbum"
          >
            <svg width="22" height="22" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            {{ t('hub.qrAlbum') }}
          </button>
        </div>
      </template>

      <input
        ref="albumInputRef"
        type="file"
        accept="image/*"
        class="hidden"
        @change="handleAlbumFile"
      />

      <!-- 错误态（相机不可用 / 二维码无效 / 相册无码）：写明原因 + 重试/返回 -->
      <div
        v-if="errorMessage"
        class="absolute inset-0 flex flex-col items-center justify-center px-8"
        :style="{ background: 'var(--mobile-overlay-heavy)' }"
      >
        <svg width="46" height="46" class="mb-4" :style="{ color: 'var(--mobile-warning)' }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <p class="text-base mb-2 text-center" :style="{ color: 'var(--mobile-warning)' }">
          {{ t('hub.qrFailed') }}
        </p>
        <p class="text-sm text-center mb-2" :style="{ color: 'var(--mobile-text-secondary)' }">
          {{ t(errorMessage) }}
        </p>
        <p
          v-if="isCameraError"
          class="text-xs text-center mb-6"
          :style="{ color: 'var(--mobile-text-secondary)' }"
        >
          {{ t('hub.qrCameraPermissionHint') }}
        </p>
        <div class="flex gap-3">
          <button
            type="button"
            class="min-h-[40px] px-4 rounded-xl text-sm font-medium"
            :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
            @click="$emit('close')"
          >
            {{ t('hub.qrBack') }}
          </button>
          <button
            type="button"
            class="min-h-[40px] px-4 rounded-xl text-sm font-medium"
            :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
            @click="retry"
          >
            {{ t('hub.qrRescan') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 二维码扫描面板（票 2026-10-09：旧宿主 `src/components/ScanPanel.vue` 能力迁入插件）
 *
 * 行为对齐（与旧实现同口径）：
 * - 相机（facingMode: environment）常驻扫描；识别到**合法**载荷即停扫并 emit `scan-result`
 * - 无码/无效码不打断相机，只在面板内给错误态；错误原因区分「无效 / 信息不完整 / 相机不可用」
 * - 相册识别（`scanFile`）：未识别 → toast 提示且保留相机扫描
 * - 照明（torch）经 `applyVideoConstraints`；设备不支持 → 禁用按钮 + toast
 *
 * 机制对齐（旧前端机制 → 插件）：
 * - 国际化：文案一律 `useHostPage().t`（键 `hub.qr*`），错误槽位存 key，渲染走 `t()`
 * - 日志：`context.logger` 带 `[host]` 前缀；catch 内必须落日志（禁止静默 catch）
 * - 安全：**不解码内容原文**（二维码含 token，C4 凭据零过境）——只记解析结论
 *
 * 未迁：旧面板的「我的二维码」信息弹窗（展示本机平台 + 默认端口，依赖宿主平台/设置
 * 投影，插件侧暂无平台投影）——留待后续按需补 mobileApi 投影，见 ticket 遗留项。
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { Html5Qrcode } from 'html5-qrcode'
import { useHostPage } from '../useHostPage'
import { parseQrText, type QrPayload } from '../qr'

const emit = defineEmits<{
  close: []
  'scan-result': [payload: QrPayload]
}>()

const ctrl = useHostPage()
const t = ctrl.t
const logger = ctrl.logger

/** 每个实例一个容器 id（html5-qrcode 按 id 找挂载点；同屏只应有一个扫描面板） */
let instanceSeq = 0
const readerId = `hub-qr-reader-${++instanceSeq}`

const readerRef = ref<HTMLElement | null>(null)
const albumInputRef = ref<HTMLInputElement | null>(null)
/** 错误槽位：i18n key（渲染一律 t()），空串 = 无错误 */
const errorMessage = ref('')
const isCameraError = ref(false)
const torchOn = ref(false)
const torchUnsupported = ref(false)

let scanner: Html5Qrcode | null = null

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

/** 停扫（幂等）：相机未启动 / 已停时静默返回；停失败只记日志（不影响返回/重试） */
async function stopScanner(): Promise<void> {
  if (!scanner?.isScanning) {
    scanner = null
    return
  }
  try {
    await scanner.stop()
  } catch (e) {
    logger.warn(`[host] qr scanner stop failed: ${errText(e)}`)
  }
  scanner = null
}

/** 识别回调：先停扫，再校验载荷；非法载荷留在面板内出错误态（不打断用户下一步重试） */
async function handleDecoded(decodedText: string): Promise<void> {
  await stopScanner()
  if (albumInputRef.value) albumInputRef.value.value = ''

  const result = parseQrText(decodedText)
  if (!result.ok) {
    // 不落二维码原文（可能含 token）：只记结论
    logger.warn(`[host] qr payload rejected: reason=${result.reason}`)
    errorMessage.value = result.reason === 'malformed' ? 'hub.qrInvalid' : 'hub.qrInvalidData'
    return
  }
  emit('scan-result', result.payload)
}

/** 启动相机扫描（重试路径先停旧实例，避免重复 start 抛错） */
async function startScanner(): Promise<void> {
  if (!readerRef.value) return
  await stopScanner()
  try {
    scanner = new Html5Qrcode(readerId)
    await scanner.start(
      { facingMode: 'environment' },
      { fps: 10, qrbox: { width: 250, height: 250 } },
      (text) => {
        void handleDecoded(text)
      },
      () => {
        // 逐帧未识别：属正常扫描过程，不记日志（热路径克制）
      },
    )
  } catch (e) {
    logger.error(`[host] qr camera start failed: ${errText(e)}`)
    isCameraError.value = true
    errorMessage.value = 'hub.qrCameraFailed'
  }
}

/** 重新扫描：清错误态并重启相机（torch 支持性重新探测） */
function retry(): void {
  errorMessage.value = ''
  isCameraError.value = false
  torchUnsupported.value = false
  torchOn.value = false
  void startScanner()
}

/** 切换照明：失败即判设备不支持（禁用按钮 + toast），避免反复失败 */
async function toggleTorch(): Promise<void> {
  if (!scanner || torchUnsupported.value) return
  const next = !torchOn.value
  try {
    await scanner.applyVideoConstraints({ advanced: [{ torch: next }] as never })
    torchOn.value = next
  } catch (e) {
    logger.warn(`[host] qr torch unsupported: ${errText(e)}`)
    torchUnsupported.value = true
    ctrl.toast.showToast(t('hub.qrTorchUnsupported'), 'warning')
  }
}

function pickFromAlbum(): void {
  albumInputRef.value?.click()
}

/** 相册图片识别：未识别只 toast（保留相机扫描，不打断） */
async function handleAlbumFile(e: Event): Promise<void> {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  try {
    if (!scanner) scanner = new Html5Qrcode(readerId)
    const decoded = await scanner.scanFile(file, true)
    if (decoded) await handleDecoded(decoded)
    else ctrl.toast.showToast(t('hub.qrAlbumNoQr'), 'warning')
  } catch (e) {
    logger.warn(`[host] qr album scan failed: ${errText(e)}`)
    ctrl.toast.showToast(t('hub.qrAlbumNoQr'), 'warning')
  } finally {
    input.value = ''
  }
}

onMounted(() => {
  void startScanner()
})

onUnmounted(() => {
  void stopScanner()
})
</script>

<style scoped>
/* 扫描线动画：取景框内自上而下循环（2s），与旧 ScanPanel 视觉一致 */
.hub-scan-line {
  position: absolute;
  left: 0.75rem;
  right: 0.75rem;
  top: 0.75rem;
  height: 2px;
  border-radius: 1px;
  background: linear-gradient(90deg, transparent, var(--mobile-accent), transparent);
  animation: hub-scan-line-move 2s ease-in-out infinite;
}

@keyframes hub-scan-line-move {
  0%,
  100% {
    top: 0.75rem;
  }
  50% {
    top: calc(100% - 0.875rem);
  }
}
</style>
