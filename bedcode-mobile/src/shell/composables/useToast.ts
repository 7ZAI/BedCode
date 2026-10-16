/**
 * Toast 反馈 — 壳内机制副本
 * -----------------------------------------------------------------------------
 * 迁移自旧机制 `src/composables/useToast.ts`（基于 vue-sonner）。
 *
 * 为什么复制：壳（新界面）必须在旧目录退役后仍能自足运行；同时避免「改旧组件
 * 顺手改掉新界面反馈行为」。契约与旧实现一致（show / success / error / warning
 * / info / dismiss / dismissAll），旧页面迁移只换 import 路径。
 *
 * 唯一差异：`ToastOptions` 由旧 `@/composables/model` 内联到本文件——壳不依赖
 * 旧类型集合，类型随机制一起自持（形状与旧定义逐字一致）。
 */

import { toast } from 'vue-sonner'

/** Toast 选项（形状与旧 `@/composables/model::ToastOptions` 一致） */
export interface ToastOptions {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
  duration?: number
  position?: 'top' | 'bottom'
}

/**
 * 本地 position 映射到 vue-sonner 位置。
 * 移动端（<600px）sonner 会自动渲染为全宽 sheet 并适配安全区，此处只需映射中轴位置。
 */
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
