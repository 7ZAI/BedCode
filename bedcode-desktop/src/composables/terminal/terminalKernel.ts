/**
 * 终端内核共享上下文（TerminalPreview 拆分产物）
 *
 * TerminalPreview.vue 拆分为多个域 composable（写入管线 / 滚动 / 渲染器 /
 * resize / 设置同步）后，xterm 实例、平台标记、模板挂载点等跨域共享状态与
 * 跨域回调统一经本上下文交换。回调在调用时经 ctx.callbacks 解析（各域创建
 * 顺序无关），避免域之间的循环依赖——JS 函数在调用时解析自由变量，只要所有
 * 域在组件 setup 中创建完成后再触发任何调用即可（实际触发点都在 initTerminal
 * / onMounted / watch 回调中，全部晚于 setup）。
 */
import { ref, shallowRef, type Ref, type ShallowRef } from 'vue'
import type { Terminal } from '@xterm/xterm'
import type { FitAddon } from '@xterm/addon-fit'
import type { WebglAddon } from '@xterm/addon-webgl'
import type { SessionInfo } from '@/stores/session'

/** 跨域回调注册表：各域创建时挂载自身实现，消费方在调用时经 ctx.callbacks 取用 */
export interface TerminalKernelCallbacks {
  /** 构造当前主题（设置同步域） */
  getTheme: () => object
  /** 将 xterm 网格同步到 PTY（resize 裁决域） */
  syncTerminalSize: () => void
  /** 应用一次 resize（resize 裁决域，含 DPR fit 与 PTY 同步） */
  applyResize: () => void
  /** DPR 感知的网格重算（渲染器域） */
  applyDprFit: () => void
  /** fit 后刷新 + 重算网格（渲染器域） */
  fitAndRefresh: () => void
  /** 透明度切换时重建渲染器（渲染器域） */
  rebuildRenderer: () => void
  /** rAF 合并的滚动到底（滚动域） */
  scrollToBottom: () => void
}

/** 终端内核共享上下文：所有域 composable 的第一个参数 */
export interface TerminalKernelContext {
  /** 当前 xterm 实例（initTerminal 创建，unmount 销毁） */
  terminalRef: ShallowRef<Terminal | null>
  /** 当前 FitAddon 实例（渲染器 / 设置同步域共享） */
  fitAddonRef: ShallowRef<FitAddon | null>
  /** 当前 WebGL addon（渲染器域读写） */
  webglAddonRef: ShallowRef<WebglAddon | null>
  /** xterm 挂载点（模板 ref） */
  terminalHostRef: Ref<HTMLElement | null>
  /** Linux 平台标记（onMounted 中 await initPlatform 后确定） */
  isLinux: Ref<boolean>
  /** 用户是否离开底部（滚动域读写，写入管线 onData 只读） */
  isUserScrolling: Ref<boolean>
  /** 已解析的背景图片 URL（设置同步域写，渲染器 computed 读） */
  bgImageUrl: Ref<string>
  /** 获取当前会话（组件 props 投影） */
  getSession: () => SessionInfo | null | undefined
  callbacks: TerminalKernelCallbacks
}

const noop = () => {}

/**
 * 创建终端内核上下文。getSession 由组件传入（读取 props.session），
 * terminalHostRef 为组件模板 ref；其余响应式状态在上下文内创建。
 */
export function createTerminalKernel(
  getSession: () => SessionInfo | null | undefined,
  terminalHostRef: Ref<HTMLElement | null>,
): TerminalKernelContext {
  return {
    terminalRef: shallowRef<Terminal | null>(null),
    fitAddonRef: shallowRef<FitAddon | null>(null),
    webglAddonRef: shallowRef<WebglAddon | null>(null),
    terminalHostRef,
    isLinux: ref(false),
    isUserScrolling: ref(false),
    bgImageUrl: ref(''),
    getSession,
    callbacks: {
      getTheme: () => ({}) as object,
      syncTerminalSize: noop,
      applyResize: noop,
      applyDprFit: noop,
      fitAndRefresh: noop,
      rebuildRenderer: noop,
      scrollToBottom: noop,
    },
  }
}
