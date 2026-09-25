/**
 * 终端 resize 裁决域（插件侧，自宿主 TerminalPreview 拆分产物迁入）
 *
 * 桌面端与移动端共用同一 PTY 尺寸：服务端裁决本端是否正统渲染端。
 * 非正统时 resize 返回 needsConfirmation（未应用），此处弹窗确认，
 * 确认后 force 重发覆盖；取消则记下被拒尺寸，防 RO/resize 事件风暴。
 *
 * 与宿主版本差异（票 01b 迁入适配）：`requestResize` 实现经参数注入——
 * 宿主版直调 `useSessionStore().resizeSession`（宿主 store）；插件版由调用方
 * 注入等价实现（经插件 WASM 命令面 `session.action.resize` 走 host-session
 * resize 原语，或开发期 mock）。裁决语义（force/needsConfirmation/拒绝抑制）
 * 逐字等价。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

/** 渲染端来源：桌面端 / 移动端设备（resize 裁决展示用；与宿主 RendererSource 同形） */
export type RendererSource = { kind: 'desktop' } | { kind: 'mobile'; deviceName: string }

/** resize 裁决结果（与宿主 ResizeOutcome serde 形状对齐） */
export type ResizeOutcome =
  | { status: 'applied'; canonical: RendererSource }
  | { status: 'needsConfirmation'; currentCanonical: RendererSource }

/** resize 请求实现（调用方注入：插件 WASM 命令面或 mock） */
export type ResizeRequester = (
  sessionId: string,
  cols: number,
  rows: number,
  force: boolean,
) => Promise<ResizeOutcome>

export function useTerminalResize(ctx: TerminalKernelContext, requestResizeImpl: ResizeRequester) {
  const { t } = useI18n()

  /** 用户拒绝覆盖的尺寸（成功后清空；同尺寸不再重发） */
  let rejectedSize: { cols: number; rows: number } | null = null

  /** 待确认的覆盖目标（弹窗内容源） */
  const rendererOverrideTarget = ref<{
    cols: number
    rows: number
    rendererName: string
  } | null>(null)
  const showRendererOverrideModal = ref(false)

  /** 渲染端显示名：桌面端用 i18n 标签，移动端用设备名；source 异常时兜底，避免弹窗崩溃 */
  function rendererDisplayName(source?: RendererSource | null): string {
    if (!source || source.kind === 'desktop') return t('session.terminal.rendererDesktop')
    return source.deviceName || t('session.terminal.rendererMobile')
  }

  /**
   * 请求调整会话尺寸（经服务端正统渲染端裁决）
   *
   * force=false：本端非正统且用户未确认前，服务端不改底层 PTY；
   * needsConfirmation 时弹出确认框。用户拒绝过的同尺寸直接忽略，
   * 避免拖窗/RO 事件持续触发弹窗风暴。
   */
  async function requestResize(cols: number, rows: number, force = false) {
    const session = ctx.getSession()
    if (!session) return
    // 用户刚拒绝过的相同尺寸：抑制（每次成功应用后清空）
    if (!force && rejectedSize && rejectedSize.cols === cols && rejectedSize.rows === rows) {
      return
    }
    // 相同尺寸已在确认弹窗中：避免并发弹窗
    if (
      rendererOverrideTarget.value &&
      rendererOverrideTarget.value.cols === cols &&
      rendererOverrideTarget.value.rows === rows
    ) {
      return
    }
    const outcome = await requestResizeImpl(session.id, cols, rows, force)
    if (outcome.status === 'applied') {
      rejectedSize = null
      return
    }
    // needsConfirmation：当前有另一个端在渲染，弹窗确认是否覆盖
    rendererOverrideTarget.value = {
      cols,
      rows,
      rendererName: rendererDisplayName(outcome.currentCanonical),
    }
    showRendererOverrideModal.value = true
  }

  /** 用户确认覆盖：force 重发（尺寸移交服务端正统归属） */
  async function confirmRendererOverride() {
    const target = rendererOverrideTarget.value
    showRendererOverrideModal.value = false
    rendererOverrideTarget.value = null
    if (target) await requestResize(target.cols, target.rows, true)
  }

  /** 用户拒绝覆盖：记录被拒尺寸，同尺寸不再打扰 */
  function cancelRendererOverride() {
    const target = rendererOverrideTarget.value
    showRendererOverrideModal.value = false
    rendererOverrideTarget.value = null
    if (target) rejectedSize = { cols: target.cols, rows: target.rows }
  }

  /** 同步当前终端尺寸到后端会话（PTY cols/rows） */
  function syncTerminalSize() {
    const terminal = ctx.terminalRef.value
    const session = ctx.getSession()
    if (!terminal || !session) return
    const cols = terminal.cols
    const rows = terminal.rows
    if (cols > 0 && rows > 0) {
      requestResize(cols, rows)
    }
  }

  /**
   * resize 实际应用（防抖器 onApply 接线）：DPR 感知 fit + 仅 cols/rows
   * 实际变化才全量重绘 + PTY 同步。subpixel 抖动（容器尺寸微调但行列不变）
   * 不触发多余重绘，避免拖动窗口时每帧全量重绘的浪费
   */
  function applyResize() {
    const terminal = ctx.terminalRef.value
    if (!terminal) return
    const cols = terminal.cols
    const rows = terminal.rows
    ctx.callbacks.applyDprFit()
    if (terminal.cols !== cols || terminal.rows !== rows) {
      terminal.refresh(0, terminal.rows - 1)
      syncTerminalSize()
    }
  }

  // 注册跨域回调（调用时解析，创建顺序无关）
  ctx.callbacks.syncTerminalSize = syncTerminalSize
  ctx.callbacks.applyResize = applyResize

  return {
    rendererOverrideTarget,
    showRendererOverrideModal,
    requestResize,
    confirmRendererOverride,
    cancelRendererOverride,
    syncTerminalSize,
    applyResize,
  }
}
