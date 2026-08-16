//! Toast notification composable — 基于 vue-sonner（Sonner 的 Vue 移植，Tailwind 风格、与移动端一致）

import { toast } from 'vue-sonner'

export interface ToastOptions {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
  duration?: number
  position?: 'top' | 'bottom'
}

/** 本地 position 映射到 vue-sonner 位置（桌面端固定中轴显示） */
function mapPosition(position: ToastOptions['position']): 'top-center' | 'bottom-center' {
  return position === 'bottom' ? 'bottom-center' : 'top-center'
}

/** 按类型分发到 sonner 对应方法（richColors 下各自有独立的等级配色与图标） */
const typeDispatch = {
  success: toast.success,
  error: toast.error,
  warning: toast.warning,
  info: toast.info,
} as const

export function useToast() {
  function show(options: ToastOptions) {
    return typeDispatch[options.type ?? 'info'](options.message, {
      duration: options.duration,
      position: mapPosition(options.position),
    })
  }

  function success(message: string, duration = 3000) {
    return show({ message, type: 'success', duration })
  }

  function error(message: string, duration = 5000) {
    return show({ message, type: 'error', duration })
  }

  function warning(message: string, duration = 4000) {
    return show({ message, type: 'warning', duration })
  }

  function info(message: string, duration = 3000) {
    return show({ message, type: 'info', duration })
  }

  function dismiss(id?: number | string) {
    toast.dismiss(id)
  }

  function dismissAll() {
    toast.dismiss()
  }

  return {
    show,
    success,
    error,
    warning,
    info,
    dismiss,
    dismissAll,
  }
}
