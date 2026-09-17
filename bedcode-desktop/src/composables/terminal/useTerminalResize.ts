/**
 * 终端 resize 裁决域（TerminalPreview 拆分产物）
 *
 * 桌面端与移动端共用同一 PTY 尺寸：服务端裁决本端是否正统渲染端。
 * 非正统时 resize 返回 needsConfirmation（未应用），此处弹窗确认，
 * 确认后 force 重发覆盖；取消则记下被拒尺寸，防 RO/resize 事件风暴。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSessionStore } from '@/stores/session'
import type { RendererSource } from '@/composables/useDesktopCommands'

export function useTerminalResize(ctx: TerminalKernelContext) {
  const sessionStore = useSessionStore()
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
    if (!source || source.kind === 'desktop') return t('desktop.terminal.rendererDesktop')
    return source.deviceName || t('desktop.terminal.rendererMobile')
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
    const outcome = await sessionStore.resizeSession(session.id, cols, rows, force)
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
