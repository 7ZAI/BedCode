/**
 * OCR plugin English translations (flat keys with ocr. domain prefix, see messages.ts)
 *
 * Kept in sync with zh-CN via the MessageSchema type.
 */
import type { MessageSchema } from './messages'

export default {
  // ==================== Toolbox entry ====================
  'ocr.toolbox.title': 'OCR Text Recognition',
  'ocr.toolbox.subtitle': 'Pick or capture an image, extract text offline',

  // ==================== Home ====================
  'ocr.home.pickAlbum': 'Pick from Gallery',
  'ocr.home.capture': 'Take Photo',
  'ocr.home.engineReady': 'Recognition engine ready',
  'ocr.home.engineLoading': 'Engine loading…',
  'ocr.home.engineUnavailable': 'Recognition engine unavailable',
  'ocr.home.modelsMissing': 'Models not installed',
  'ocr.home.modelsMissingDesc': 'Restore models first (~17MB), then recognize offline',
  'ocr.home.restoreModels': 'Restore Models',
  'ocr.home.restoring': 'Restoring…',
  'ocr.home.recognizing': 'Recognizing…',
  'ocr.home.recognizingDesc': 'Extracting text from the image…',
  'ocr.home.cancel': 'Cancel',

  // ==================== Result ====================
  'ocr.result.title': 'Recognition Result',
  'ocr.result.meta': '{count} lines · {duration} ms',
  'ocr.result.empty': 'No text detected',
  'ocr.result.emptyDesc': 'Try a clearer image',
  'ocr.result.copyAll': 'Copy All',
  'ocr.result.copy': 'Copy',
  'ocr.result.copyLine': 'Copy this line',
  'ocr.result.copyFailed': 'Copy failed, please retry',
  'ocr.result.copied': 'Copied to clipboard',
  'ocr.result.lineCopied': 'Line copied',
  'ocr.result.lowConfidence': 'Low confidence',
  'ocr.result.backToHome': 'Back to Home',

  // ==================== Settings (model management) ====================
  'ocr.settings.modelTitle': 'Recognition Models',
  'ocr.settings.modelDesc': 'PP-OCRv4 offline models (Chinese/English mixed); delete to free space, restore to re-extract',
  'ocr.settings.modelsBytes': 'Models size',
  'ocr.settings.deleteModels': 'Delete Models',
  'ocr.settings.restoreModels': 'Restore Models',
  'ocr.settings.deleting': 'Deleting…',
  'ocr.settings.restoring': 'Restoring…',
  'ocr.settings.deleteConfirm': 'Delete recognition models? Recognition requires restoring them later.',
  'ocr.settings.deleted': 'Models deleted',
  'ocr.settings.restored': 'Models restored',
  'ocr.settings.restoreFailed': 'Failed to restore models, please retry',
  'ocr.settings.deleteFailed': 'Failed to delete models, please retry',

  // ==================== Errors ====================
  'ocr.error.pickFailed': 'Failed to pick image',
  'ocr.error.captureTimeout': 'Image capture timed out, please retry',
  'ocr.error.decodeFailed': 'Failed to decode image, try another one',
  'ocr.error.recognizeFailed': 'Recognition failed',
  'ocr.error.permissionDenied': 'Camera permission denied. Allow it in system settings and retry.',
  'ocr.error.noCameraApp': 'No camera app available',
} satisfies MessageSchema
