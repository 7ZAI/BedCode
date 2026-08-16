/**
 * OCR 插件 i18n 消息 schema（扁平 key，`{domain}.{section}.{key}`；zh-CN/en 必须同步）
 *
 * 注意：必须与 file-transfer 同为扁平 key 且 key 含 `ocr.` 域前缀——
 * dev-shell 与宿主的 registerMessages 按顶层 entry 加 `{pluginId}.` 前缀，
 * t('ocr.xxx') 查找 `{pluginId}.ocr.xxx`；嵌套对象或缺域前缀都会导致
 * 查找不到（渲染出原始 key）。
 */
export interface MessageSchema {
  'ocr.toolbox.title': string
  'ocr.toolbox.subtitle': string
  'ocr.home.pickAlbum': string
  'ocr.home.capture': string
  'ocr.home.engineReady': string
  'ocr.home.engineLoading': string
  'ocr.home.engineUnavailable': string
  'ocr.home.modelsMissing': string
  'ocr.home.modelsMissingDesc': string
  'ocr.home.restoreModels': string
  'ocr.home.restoring': string
  'ocr.home.recognizing': string
  'ocr.home.recognizingDesc': string
  'ocr.home.cancel': string
  'ocr.result.title': string
  'ocr.result.meta': string
  'ocr.result.empty': string
  'ocr.result.emptyDesc': string
  'ocr.result.copyAll': string
  'ocr.result.copied': string
  'ocr.result.lineCopied': string
  'ocr.result.lowConfidence': string
  'ocr.result.backToHome': string
  'ocr.settings.modelTitle': string
  'ocr.settings.modelDesc': string
  'ocr.settings.modelsBytes': string
  'ocr.settings.deleteModels': string
  'ocr.settings.restoreModels': string
  'ocr.settings.deleting': string
  'ocr.settings.restoring': string
  'ocr.settings.deleteConfirm': string
  'ocr.settings.deleted': string
  'ocr.settings.restored': string
  'ocr.settings.restoreFailed': string
  'ocr.settings.deleteFailed': string
  'ocr.error.pickFailed': string
  'ocr.error.decodeFailed': string
  'ocr.error.recognizeFailed': string
  'ocr.error.permissionDenied': string
  'ocr.error.noCameraApp': string
}
