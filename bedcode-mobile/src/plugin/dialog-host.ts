/**
 * Plugin Dialog Host
 *
 * 全局对话框服务 — 插件通过 context.dialogs 调用，宿主渲染移动端样式弹窗。
 * 模块级响应式队列 + Promise 关联，多插件并发弹窗按顺序展示。
 * 渲染组件 PluginDialogHost 由 App.vue 挂载，样式复用宿主 --mobile-* 变量。
 */

import { ref } from 'vue'
import type { DialogOptions, DialogResult } from '@binblink/plugin-sdk-mobile'
import { useToast } from '@/composables/useToast'

/** 队列中的对话框条目 */
export interface DialogItem {
  id: number
  kind: 'dialog' | 'confirm' | 'prompt'
  options: DialogOptions
  resolve: (result: DialogResult) => void
}

/** 模块级队列（跨组件共享单例） */
const queue = ref<DialogItem[]>([])
let nextId = 0

/** 宿主全局轻提示（useToast 基于 vue-sonner，App.vue 挂载 Toaster 渲染） */
const toast = useToast()

function push(kind: DialogItem['kind'], options: DialogOptions): Promise<DialogResult> {
  return new Promise<DialogResult>(resolve => {
    const id = ++nextId
    queue.value.push({ id, kind, options, resolve })
  })
}

/** 解析队列顶部条目 */
function resolveTop(action: DialogResult['action'], value?: string): void {
  const item = queue.value.shift()
  if (item) item.resolve({ action, value })
}

/** 插件对话框服务（暴露到 window.__BEDCODE_SHARED__.dialogs） */
export const pluginDialogHost = {
  queue,
  showDialog: (options: DialogOptions) => push('dialog', options),
  showConfirm: (options: DialogOptions) =>
    push('confirm', options).then(r => r.action === 'confirm'),
  showPrompt: (options: DialogOptions) =>
    push('prompt', options).then(r => (r.action === 'confirm' ? (r.value ?? '') : null)),
  showToast: (message: string, type: 'info' | 'success' | 'warning' | 'error' = 'info') => {
    // 错误/警告需要更长的可读时间，与 useToast 内置方法默认时长一致
    const duration = type === 'error' ? 5000 : type === 'warning' ? 4000 : 3000
    toast.show({ message, type, duration })
  },
  resolveTop,
}
