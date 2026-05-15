//! Error Handler Composable
//!
//! 统一的错误处理和用户通知

import { ref, readonly } from 'vue'

export interface AppError {
  code: string
  message: string
  timestamp: Date
  details?: unknown
}

type ErrorSeverity = 'error' | 'warning' | 'info'

interface ErrorOptions {
  severity?: ErrorSeverity
  duration?: number
  details?: unknown
}

// 全局错误状态
const errors = ref<AppError[]>([])
const lastError = ref<AppError | null>(null)

// 错误码映射
const ERROR_MESSAGES: Record<string, string> = {
  // 后端错误码
  'PTY_ERROR': '终端进程错误',
  'SESSION_ERROR': '会话错误',
  'DATABASE_ERROR': '数据库错误',
  'IO_ERROR': '文件操作错误',
  'SERIALIZATION_ERROR': '数据序列化错误',
  'WEBSOCKET_ERROR': 'WebSocket 连接错误',
  'AUTH_ERROR': '认证错误',
  'DISCOVERY_ERROR': '设备发现错误',
  'CONFIG_ERROR': '配置错误',
  'PARSE_ERROR': '解析错误',
  'NOTIFICATION_ERROR': '通知错误',
  'KEYRING_ERROR': '密钥存储错误',
  'NOT_FOUND': '资源未找到',
  'INVALID_INPUT': '输入无效',
  'INTERNAL_ERROR': '内部错误',

  // 前端错误码
  'NETWORK_ERROR': '网络连接失败',
  'TIMEOUT_ERROR': '操作超时',
  'PERMISSION_ERROR': '权限不足',
  'UNKNOWN_ERROR': '未知错误',
}

/**
 * 解析后端错误
 */
function parseBackendError(error: unknown): AppError {
  if (typeof error === 'string') {
    // 尝试解析错误字符串
    const match = error.match(/^(\w+):\s*(.+)$/)
    if (match) {
      return {
        code: match[1],
        message: match[2],
        timestamp: new Date(),
      }
    }
    return {
      code: 'UNKNOWN_ERROR',
      message: error,
      timestamp: new Date(),
    }
  }

  if (error instanceof Error) {
    return {
      code: 'UNKNOWN_ERROR',
      message: error.message,
      timestamp: new Date(),
      details: error,
    }
  }

  return {
    code: 'UNKNOWN_ERROR',
    message: String(error),
    timestamp: new Date(),
    details: error,
  }
}

/**
 * 获取用户友好的错误消息
 */
function getErrorMessage(code: string, originalMessage: string): string {
  const friendlyMessage = ERROR_MESSAGES[code]
  return friendlyMessage ? `${friendlyMessage}: ${originalMessage}` : originalMessage
}

/**
 * 错误处理器
 */
export function useErrorHandler() {
  /**
   * 处理错误
   */
  function handleError(error: unknown, options: ErrorOptions = {}): AppError {
    const appError = parseBackendError(error)
    const friendlyMessage = getErrorMessage(appError.code, appError.message)

    const fullError: AppError = {
      ...appError,
      message: friendlyMessage,
      details: options.details || appError.details,
    }

    // 记录错误
    errors.value.push(fullError)
    lastError.value = fullError

    // 控制台输出
    const severity = options.severity || 'error'
    const logMethod = severity === 'error' ? console.error
                    : severity === 'warning' ? console.warn
                    : console.info

    logMethod(`[${severity.toUpperCase()}] ${appError.code}: ${appError.message}`, error)

    // 自动移除旧错误（保留最近 50 条）
    if (errors.value.length > 50) {
      errors.value = errors.value.slice(-50)
    }

    return fullError
  }

  /**
   * 处理异步操作的错误
   */
  async function withErrorHandling<T>(
    operation: () => Promise<T>,
    options: ErrorOptions = {}
  ): Promise<{ data?: T; error?: AppError }> {
    try {
      const data = await operation()
      return { data }
    } catch (e) {
      const error = handleError(e, options)
      return { error }
    }
  }

  /**
   * 清除错误
   */
  function clearErrors() {
    errors.value = []
    lastError.value = null
  }

  /**
   * 清除特定错误
   */
  function clearError(index: number) {
    errors.value.splice(index, 1)
    if (errors.value.length === 0) {
      lastError.value = null
    }
  }

  return {
    errors: readonly(errors),
    lastError: readonly(lastError),
    handleError,
    withErrorHandling,
    clearErrors,
    clearError,
  }
}

/**
 * Toast 通知
 */
export function useToast() {
  interface Toast {
    id: string
    message: string
    type: 'success' | 'error' | 'warning' | 'info'
    duration: number
  }

  const toasts = ref<Toast[]>([])

  function show(message: string, type: Toast['type'] = 'info', duration = 3000) {
    const id = Date.now().toString()
    const toast: Toast = { id, message, type, duration }

    toasts.value.push(toast)

    if (duration > 0) {
      setTimeout(() => {
        remove(id)
      }, duration)
    }

    return id
  }

  function remove(id: string) {
    const index = toasts.value.findIndex(t => t.id === id)
    if (index !== -1) {
      toasts.value.splice(index, 1)
    }
  }

  function success(message: string, duration?: number) {
    return show(message, 'success', duration)
  }

  function error(message: string, duration?: number) {
    return show(message, 'error', duration)
  }

  function warning(message: string, duration?: number) {
    return show(message, 'warning', duration)
  }

  function info(message: string, duration?: number) {
    return show(message, 'info', duration)
  }

  return {
    toasts: readonly(toasts),
    show,
    remove,
    success,
    error,
    warning,
    info,
  }
}
