/**
 * 壳内 Toast 反馈（与宿主 shell/composables/useToast.ts 同款 API）
 * -----------------------------------------------------------------------------
 * 宿主用 vue-sonner 渲染 toast，dev-shell 没有该依赖，且已有自己的对话框宿主
 * （`mock/dialog-service.ts` + `components/DialogHost.vue`）在渲染同款队列与 toast。
 * 因此这里只做一层薄适配：壳内组件用宿主同款 API 调用，落点换成 dev-shell 的
 * DialogHost——不为预览环境引入第二套 toast 实现。
 *
 * API 形状（show / success / error / warning / info / dismiss）保持一致，便于
 * 「同款代码在两边跑」。
 */

import { dialogService } from '../../mock/dialog-service'

/** Toast 选项（与宿主 useToast 的 ToastOptions 同形） */
export interface ToastOptions {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
}

export function useToast() {
  function show(options: ToastOptions) {
    dialogService.showToast(options.message, options.type ?? 'info')
  }

  return {
    show,
    success: (message: string) => show({ message, type: 'success' }),
    error: (message: string) => show({ message, type: 'error' }),
    warning: (message: string) => show({ message, type: 'warning' }),
    info: (message: string) => show({ message, type: 'info' }),
  }
}