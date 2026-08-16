/**
 * OCR 插件中文翻译（默认语言；扁平 key 含 ocr. 域前缀，见 messages.ts）
 */
import type { MessageSchema } from './messages'

export default {
  // ==================== 工具箱入口 ====================
  'ocr.toolbox.title': 'OCR 文字识别',
  'ocr.toolbox.subtitle': '相册/拍照取图，离线识别文本',

  // ==================== 主页 ====================
  'ocr.home.pickAlbum': '相册选图',
  'ocr.home.capture': '拍照',
  'ocr.home.engineReady': '识别引擎已就绪',
  'ocr.home.engineLoading': '引擎加载中…',
  'ocr.home.engineUnavailable': '识别引擎不可用',
  'ocr.home.modelsMissing': '模型未安装',
  'ocr.home.modelsMissingDesc': '首次使用需先恢复模型（约 17MB），完成后即可离线识别',
  'ocr.home.restoreModels': '恢复模型',
  'ocr.home.restoring': '恢复中…',
  'ocr.home.recognizing': '识别中…',
  'ocr.home.recognizingDesc': '请稍候，正在提取图片中的文字',
  'ocr.home.cancel': '取消',

  // ==================== 结果页 ====================
  'ocr.result.title': '识别结果',
  'ocr.result.meta': '{count} 行 · {duration} ms',
  'ocr.result.empty': '未识别到文字',
  'ocr.result.emptyDesc': '换一张更清晰的图片再试试',
  'ocr.result.copyAll': '复制全文',
  'ocr.result.copied': '已复制到剪贴板',
  'ocr.result.lineCopied': '已复制该行',
  'ocr.result.lowConfidence': '低置信度',
  'ocr.result.backToHome': '返回主页',

  // ==================== 设置区（模型管理） ====================
  'ocr.settings.modelTitle': '识别模型',
  'ocr.settings.modelDesc': 'PP-OCRv4 离线模型（中文/英文混排）；删除后释放空间，恢复后重新解压',
  'ocr.settings.modelsBytes': '模型占用',
  'ocr.settings.deleteModels': '删除模型',
  'ocr.settings.restoreModels': '恢复模型',
  'ocr.settings.deleting': '删除中…',
  'ocr.settings.restoring': '恢复中…',
  'ocr.settings.deleteConfirm': '确定删除识别模型？删除后需重新恢复才能识别。',
  'ocr.settings.deleted': '模型已删除',
  'ocr.settings.restored': '模型已恢复',
  'ocr.settings.restoreFailed': '模型恢复失败，请重试',
  'ocr.settings.deleteFailed': '模型删除失败，请重试',

  // ==================== 错误 ====================
  'ocr.error.pickFailed': '取图失败',
  'ocr.error.decodeFailed': '图片解码失败，请换一张图片',
  'ocr.error.recognizeFailed': '识别失败',
  'ocr.error.permissionDenied': '相机权限被拒绝，请在系统设置中允许后重试',
  'ocr.error.noCameraApp': '未找到可用的相机应用',
} satisfies MessageSchema
