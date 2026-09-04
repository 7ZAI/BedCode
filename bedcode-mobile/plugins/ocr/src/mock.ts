/**
 * OCR 插件 dev-shell 领域数据
 *
 * ocrLinesSeed：ocr.recognize 的 mock 返回（dev-shell 消费）；
 * dev-shell 不含业务示例数据，缺省/空数组均演示「未识别到文字」空态。
 * 真实宿主忽略此导出（对 activate 无影响）。
 */
import type { PluginDevMock, OcrLine } from '@binblink/plugin-sdk-mobile'

/** OCR 识别结果种子（dev-shell ocr.recognize mock 返回；插件自有类型，SDK 不收录） */
export type OcrLinesSeed = OcrLine[]

/** 示例识别结果（中英混排 + 低置信度行演示弱化） */
export const defaultOcrLinesSeed: OcrLinesSeed = [
  { text: 'Hello, BedCode OCR', confidence: 0.98, bbox: { x: 24, y: 40, w: 420, h: 36 } },
  { text: '离线文字识别（PP-OCRv4）', confidence: 0.95, bbox: { x: 24, y: 92, w: 380, h: 40 } },
  { text: '低置信度示例行（点击可复制）', confidence: 0.52, bbox: { x: 24, y: 148, w: 300, h: 36 } },
]

/**
 * dev-shell 可经「领域数据」面板切换种子：
 * - 缺省：defaultOcrLinesSeed（混合演示）
 * - 空数组：演示空结果空态
 */
export const devMock: PluginDevMock = {
  ocrLinesSeed: defaultOcrLinesSeed,
}
