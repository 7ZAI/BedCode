/**
 * 错误码 → i18n key 映射
 *
 * 后端 Rust 返回的错误码（如 PTY_ERROR、AUTH_ERROR）需要翻译为用户友好的消息。
 * 此映射表将错误码对应到 i18n 翻译 key，消费端通过 i18n.global.t() 获取翻译文本。
 */
export const ERROR_CODE_I18N_KEY: Record<string, string> = {
  // 后端错误码
  PTY_ERROR: 'common.errorCode.ptyError',
  SESSION_ERROR: 'common.errorCode.sessionError',
  DATABASE_ERROR: 'common.errorCode.databaseError',
  IO_ERROR: 'common.errorCode.ioError',
  SERIALIZATION_ERROR: 'common.errorCode.serializationError',
  WEBSOCKET_ERROR: 'common.errorCode.websocketError',
  AUTH_ERROR: 'common.errorCode.authError',
  DISCOVERY_ERROR: 'common.errorCode.discoveryError',
  CONFIG_ERROR: 'common.errorCode.configError',
  PARSE_ERROR: 'common.errorCode.parseError',
  NOTIFICATION_ERROR: 'common.errorCode.notificationError',
  KEYRING_ERROR: 'common.errorCode.keyringError',
  NOT_FOUND: 'common.errorCode.notFound',
  INVALID_INPUT: 'common.errorCode.invalidInput',
  INTERNAL_ERROR: 'common.errorCode.internalError',

  // 前端错误码
  NETWORK_ERROR: 'common.errorCode.networkError',
  TIMEOUT_ERROR: 'common.errorCode.timeoutError',
  PERMISSION_ERROR: 'common.errorCode.permissionError',
  UNKNOWN_ERROR: 'common.errorCode.unknownError',
  IPC_TIMEOUT: 'common.errorCode.ipcTimeout',
}
