//! Error Handler Composable
//!
//! 统一的错误处理和用户通知

import { ref, readonly } from 'vue'
import i18n from '@/locales'
import { ERROR_CODE_I18N_KEY } from '@/locales/errorCodes'

// Re-export from model
import type { AppError } from './model'
export type { AppError }

type ErrorSeverity = 'error' | 'warning' | 'info'

interface ErrorOptions {
  severity?: ErrorSeverity
  duration?: number
  details?: unknown
}

// 全局错误状态
const errors = ref<AppError[]>([])
const lastError = ref<AppError | null>(null)

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
        timestamp: Date.now(),
      }
    }
    return {
      code: 'UNKNOWN_ERROR',
      message: error,
      timestamp: Date.now(),
    }
  }

  if (error instanceof Error) {
    return {
      code: 'UNKNOWN_ERROR',
      message: error.message,
      timestamp: Date.now(),
      details: error.message,
    }
  }

  return {
    code: 'UNKNOWN_ERROR',
    message: String(error),
    timestamp: Date.now(),
    details: String(error),
  }
}

/**
 * 获取用户友好的错误消息
 */
function getErrorMessage(code: string, originalMessage: string): string {
  const i18nKey = ERROR_CODE_I18N_KEY[code]
  const friendlyMessage = i18nKey ? i18n.global.t(i18nKey) : i18n.global.t('common.errorCode.unknownError')
  return `${friendlyMessage}: ${originalMessage}`
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
      details: (options.details || appError.details) as string | undefined,
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
