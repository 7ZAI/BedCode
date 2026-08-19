/**
 * useOcr — OCR 插件业务逻辑（spec §6.2/§6.3）
 *
 * 状态机：
 * - enginePhase：unknown → (engineStatus) → ready / missing / unavailable / loading
 * - 识别流：pickImage / cameraCapture → recognize → result（module 级共享给结果页）
 * - busy 防重入：取图/识别/模型管理任一进行中，主页按钮禁用
 * - 错误映射：权限拒绝 / 无相机应用 / 解码失败 / 模型缺失 等 → i18n key
 *
 * 识别数据不经 WASM：context.ocr.* 直通宿主命令（spec §6）。
 */
import { computed, ref } from 'vue'
import type { PluginContext, OcrEngineStatus, OcrLine } from '@binblink/plugin-sdk-mobile'

/** 引擎阶段（由 engineStatus 派生） */
export type EnginePhase = 'unknown' | 'loading' | 'ready' | 'missing' | 'unavailable'

/** 低置信度阈值（spec §6.2：confidence < 0.6 行弱化） */
export const LOW_CONFIDENCE_THRESHOLD = 0.6

/** 模型缺失错误特征串（Rust 侧 plugin_ocr_recognize 的错误文案） */
const MODELS_MISSING_MARKERS = ['models not extracted', '模型未解压', '模型缺失']

/** 取图超时（ms）：相机/相册 invoke 静默挂起（如权限请求失败）时兜底复位，
 *  避免 UI 永久停留「识别中」。需覆盖正常拍照/选图等待，仅兜底异常路径（spec §6.2 防重入）。 */
export const PICK_TIMEOUT_MS = 120_000

/** 带超时等待：超时 rejected（错误串含 timed out，供 mapRecognizeError 映射）；
 *  promise 先 settle 时清理定时器，避免悬空计时器。 */
export function withTimeout<T>(promise: Promise<T>, ms: number, what: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`${what} timed out after ${ms}ms`))
    }, ms)
    promise.then(
      (v) => {
        clearTimeout(timer)
        resolve(v)
      },
      (e) => {
        clearTimeout(timer)
        reject(e)
      },
    )
  })
}

// ==================== module 级共享状态（OcrView 写入，ResultPage 读取） ====================

/** 最近一次识别结果（null = 未识别 / 已清空） */
const lastLines = ref<OcrLine[] | null>(null)
const lastDurationMs = ref(0)
/** 识别使用的图源（结果页可展示来源信息） */
const lastSource = ref<{ path: string; width: number; height: number } | null>(null)

/** 结果页路由 id（manifest contributes.routes） */
export const RESULT_ROUTE_ID = 'result'

/** 清空共享结果（结果页返回主页时调用） */
export function clearOcrResult(): void {
  lastLines.value = null
  lastDurationMs.value = 0
  lastSource.value = null
}

/**
 * 创建 OCR 业务逻辑实例（每组件调用一次；共享状态为 module 级）
 */
export function useOcr(context: PluginContext) {
  // ==================== 引擎状态 ====================
  const engineStatus = ref<OcrEngineStatus | null>(null)
  const enginePhase = computed<EnginePhase>(() => {
    const s = engineStatus.value
    if (!s) return 'unknown'
    if (!s.available) return 'unavailable'
    if (!s.modelsPresent) return 'missing'
    return s.engineLoaded ? 'ready' : 'loading'
  })

  /** 刷新引擎状态（主页挂载 / 模型恢复后调用） */
  async function refreshEngineStatus(): Promise<void> {
    try {
      engineStatus.value = await context.ocr.engineStatus()
    } catch (err) {
      context.logger.warn(`ocr.engineStatus failed: ${err}`)
      engineStatus.value = null
    }
  }

  // ==================== 识别流 ====================
  /** 识别中（取图 + 识别全程置位，防重入） */
  const recognizing = ref(false)

  /**
   * 取图并识别：source 为 null（用户取消）静默返回 false；
   * 识别成功写入共享结果并返回 true（调用方跳转结果页）。
   */
  async function pickAndRecognize(source: () => Promise<{ path: string; width: number; height: number } | null>): Promise<boolean> {
    if (recognizing.value) return false
    recognizing.value = true
    try {
      // 取图带超时兜底：宿主命令静默挂起（权限请求失败等）时不至于永久卡「识别中」
      const image = await withTimeout(source(), PICK_TIMEOUT_MS, 'ocr image source')
      if (!image) return false // 用户取消
      // 取图产物 {path} → 识别入参 {rgbaPath}（spec §4.4 调用路径）
      const result = await context.ocr.recognize({
        image: { rgbaPath: image.path, width: image.width, height: image.height },
      })
      lastLines.value = result.lines
      lastDurationMs.value = result.durationMs
      lastSource.value = image
      return true
    } finally {
      recognizing.value = false
    }
  }

  /** 相册选图 → 识别 */
  async function recognizeFromAlbum(): Promise<boolean> {
    return pickAndRecognize(() => context.ocr.pickImage())
  }

  /** 拍照 → 识别 */
  async function recognizeFromCamera(): Promise<boolean> {
    return pickAndRecognize(() => context.ocr.cameraCapture())
  }

  // ==================== 结果（module 级共享） ====================
  const lines = computed(() => lastLines.value)
  const durationMs = computed(() => lastDurationMs.value)
  const source = computed(() => lastSource.value)

  // ==================== 模型管理（设置区） ====================
  const modelBusy = ref(false)

  /** 删除模型：成功后刷新引擎状态（入口禁用由 enginePhase 派生） */
  async function deleteModels(): Promise<void> {
    if (modelBusy.value) return
    modelBusy.value = true
    try {
      await context.ocr.deleteModels()
      await refreshEngineStatus()
    } finally {
      modelBusy.value = false
    }
  }

  /** 恢复模型：成功后刷新引擎状态（恢复后入口解禁） */
  async function restoreModels(): Promise<void> {
    if (modelBusy.value) return
    modelBusy.value = true
    try {
      await context.ocr.restoreModels()
      await refreshEngineStatus()
    } finally {
      modelBusy.value = false
    }
  }

  return {
    engineStatus,
    enginePhase,
    refreshEngineStatus,
    recognizing,
    recognizeFromAlbum,
    recognizeFromCamera,
    lines,
    durationMs,
    source,
    modelBusy,
    deleteModels,
    restoreModels,
  }
}

// ==================== 错误映射（错误串 → i18n key） ====================

/** 识别命令错误 → 结果提示 key（模型缺失特判为「引导恢复」） */
export function mapRecognizeError(err: unknown): { key: string; params?: Record<string, any> } {
  const message = err instanceof Error ? err.message : String(err)
  const lower = message.toLowerCase()
  if (lower.includes('timed out')) {
    return { key: 'ocr.error.captureTimeout' }
  }
  if (MODELS_MISSING_MARKERS.some((m) => lower.includes(m.toLowerCase()))) {
    return { key: 'ocr.home.modelsMissing' }
  }
  if (lower.includes('camera permission') || lower.includes('permission denied')) {
    return { key: 'ocr.error.permissionDenied' }
  }
  if (lower.includes('no camera app')) {
    return { key: 'ocr.error.noCameraApp' }
  }
  if (lower.includes('decode') || lower.includes('corrupted') || lower.includes('unsupported')) {
    return { key: 'ocr.error.decodeFailed' }
  }
  return { key: 'ocr.error.recognizeFailed' }
}
