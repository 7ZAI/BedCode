//! Toast notification composable

import { h, ref } from 'vue'
import Toast from '@/components/Toast.vue'

// Re-export from model
import type { ToastOptions } from './model'
export type { ToastOptions }



const toasts = ref<Array<{ id: number; options: ToastOptions }>>([])
let toastId = 0

export function useToast() {
  function show(options: ToastOptions) {
    const id = ++toastId
    toasts.value.push({ id, options })

    return id
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

  function dismiss(id: number) {
    const index = toasts.value.findIndex(t => t.id === id)
    if (index !== -1) {
      toasts.value.splice(index, 1)
    }
  }

  function dismissAll() {
    toasts.value = []
  }

  return {
    toasts,
    show,
    success,
    error,
    warning,
    info,
    dismiss,
    dismissAll,
  }
}

// Toast container component for mounting in App.vue
export const ToastContainer = {
  name: 'ToastContainer',
  setup() {
    const { toasts, dismiss } = useToast()

    return () =>
      toasts.value.map(({ id, options }) =>
        h(Toast, {
          key: id,
          ...options,
          onClose: () => dismiss(id),
        })
      )
  },
}
