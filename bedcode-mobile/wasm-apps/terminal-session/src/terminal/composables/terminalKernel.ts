/**
 * 终端内核共享上下文（TerminalView 拆分产物，范式参考桌面端
 * `bedcode-desktop/src/composables/terminal/terminalKernel.ts`）
 *
 * TerminalView.vue 拆分为多个域 composable（渲染器 / resize 裁决 / 键盘避让 /
 * 订阅与历史门控）后，xterm 实例、addon 实例、模板挂载点等跨域共享状态与跨域
 * 回调统一经本上下文交换。回调在**调用时**经 ctx.callbacks 解析（各域创建顺序
 * 无关），避免域之间的循环依赖——实际触发点都在 initTerminal / onMounted / watch
 * 回调中，全部晚于 setup。
 *
 * 与桌面端的差异（移动端实情）：
 * - 终端实例的创建与销毁仍由 TerminalView 编排（构造选项含移动端专用项）
 * - 会话/连接状态以 getter 注入（移动端会话来自路由参数 + 连接 composable，
 *   不像桌面端是 props）
 * - 无 WebGL addon 引用时视为 DOM 渲染器（移动端默认，见 useTerminalRenderer）
 */
import { ref, shallowRef, type Ref, type ShallowRef } from 'vue'
import type { Terminal } from '@xterm/xterm'
import type { FitAddon } from '@xterm/addon-fit'
import type { WebglAddon } from '@xterm/addon-webgl'

/** 跨域回调注册表：各域创建时挂载自身实现，消费方在调用时经 ctx.callbacks 取用 */
export interface TerminalKernelCallbacks {
  /** DPR 感知的网格重算（渲染器域） */
  applyDprFit: () => void
  /** 应用一次 resize（resize 域：DPR fit + 条件重绘 + PTY 同步） */
  applyResize: () => void
}

const noop = () => {}

/** 终端内核共享上下文：所有域 composable 的第一个参数 */
export interface TerminalKernelContext {
  /** 当前 xterm 实例（initTerminal 创建，卸载销毁） */
  terminalRef: ShallowRef<Terminal | null>
  /** 当前 FitAddon 实例（渲染器 / resize 域共享） */
  fitAddonRef: ShallowRef<FitAddon | null>
  /** 当前 WebGL addon（渲染器域读写；mobile 默认不加载 → null = DOM 渲染器） */
  webglAddonRef: ShallowRef<WebglAddon | null>
  /** xterm 挂载点（模板 ref，渲染器域测量容器尺寸） */
  xtermContainerRef: Ref<HTMLDivElement | null>
  /**
   * 网格是否已由真实字体度量校准（applyDprFit 的非降级分支执行过）。
   * 构造期字体未就绪时 `computeInitialSize` 会回退 80x24，该尺寸不得下发 PTY；
   * 校准后即便真实网格恰为 80x24 也必须下发（resize 域消费）。
   */
  gridCalibrated: Ref<boolean>
  /** 当前会话 ID（组件路由参数投影） */
  getSessionId: () => string
  /** 终端 WS 是否已连接（组件连接状态投影） */
  isConnected: () => boolean
  /** 当前会话是否活跃（组件会话状态投影） */
  isSessionActive: () => boolean
  callbacks: TerminalKernelCallbacks
}

/**
 * 创建终端内核上下文。会话/连接状态以 getter 注入（调用时解析），
 * xtermContainerRef 为组件模板 ref；其余响应式状态在上下文内创建。
 */
export function createTerminalKernel(
  xtermContainerRef: Ref<HTMLDivElement | null>,
  getSessionId: () => string,
  isConnected: () => boolean,
  isSessionActive: () => boolean,
): TerminalKernelContext {
  return {
    terminalRef: shallowRef<Terminal | null>(null),
    fitAddonRef: shallowRef<FitAddon | null>(null),
    webglAddonRef: shallowRef<WebglAddon | null>(null),
    xtermContainerRef,
    gridCalibrated: ref(false),
    getSessionId,
    isConnected,
    isSessionActive,
    callbacks: {
      applyDprFit: noop,
      applyResize: noop,
    },
  }
}
