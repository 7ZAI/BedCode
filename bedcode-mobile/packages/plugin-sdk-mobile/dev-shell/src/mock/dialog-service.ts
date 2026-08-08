/**
 * 对话框服务 mock（宿主 dialog-host.ts 的浏览器实现）
 *
 * 队列式 Promise 关联：DialogHost.vue 渲染 queue 中的条目，
 * 用户操作后 resolveTop 完成对应 Promise。toasts 由 ToastHost 渲染。
 */
import { ref } from 'vue'
import type { DialogOptions, DialogResult } from '../../src/types'

export interface DialogItem {
  id: number
  kind: 'dialog' | 'confirm' | 'prompt'
  options: DialogOptions
  resolve: (result: DialogResult) => void
}

export interface ToastItem {
  id: number
  message: string
  type: 'info' | 'success' | 'warning' | 'error'
}

const queue = ref<DialogItem[]>([])
const toasts = ref<ToastItem[]>([])
let nextId = 0

function push(kind: DialogItem['kind'], options: DialogOptions): Promise<DialogResult> {
  return new Promise<DialogResult>((resolve) => {
    const id = ++nextId
    queue.value.push({ id, kind, options, resolve })
  })
}

/** 解析队列顶部条目 */
export function resolveTop(action: DialogResult['action'], value?: string): void {
  const item = queue.value.shift()
  if (item) item.resolve({ action, value })
}

/** 暴露到 window.__BEDCODE_SHARED__.dialogs（与宿主 pluginDialogHost 同形状） */
export const dialogService = {
  queue,
  toasts,
  showDialog: (options: DialogOptions) => push('dialog', options),
  showConfirm: (options: DialogOptions) =>
    push('confirm', options).then((r) => r.action === 'confirm'),
  showPrompt: (options: DialogOptions) =>
    push('prompt', options).then((r) => (r.action === 'confirm' ? (r.value ?? '') : null)),
  showToast: (message: string, type: ToastItem['type'] = 'info') => {
    const id = ++nextId
    toasts.value.push({ id, message, type })
    setTimeout(() => {
      const idx = toasts.value.findIndex((t) => t.id === id)
      if (idx !== -1) toasts.value.splice(idx, 1)
    }, type === 'error' ? 5000 : type === 'warning' ? 4000 : 3000)
  },
  resolveTop,
}
