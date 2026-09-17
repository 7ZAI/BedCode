/**
 * 终端 resize 裁决域（TerminalView 拆分产物，范式参考桌面端
 * `bedcode-desktop/src/composables/terminal/useTerminalResize.ts`）
 *
 * 职责：所有网格尺寸变化收敛到 HTTP 单通道串行队列（同一时刻仅一个在途请求，
 * 期间到达的新尺寸合并为最新值），并处理服务端的「正统渲染端」裁决（本端非正统
 * 时弹窗确认覆盖）。移动端比桌面端多一层：未校准的构造期兜底网格 80x24 不得下发。
 *
 * 依赖：共享内核 ctx（terminalRef / fitAddonRef / gridCalibrated /
 * getSessionId / isConnected + applyDprFit 回调）。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import { httpResizeSession, type RendererSource } from '@/composables/useHttpApi'
import { isMockSession } from '@/composables/useMockTerminal'

export function useTerminalResize(ctx: TerminalKernelContext) {
  const { t } = useI18n()

  // ==================== 串行队列状态 ====================
  let resizeInFlight = false
  let pendingResize: { cols: number; rows: number; force: boolean } | null = null

  // ==================== 正统渲染端裁决交互 ====================
  /** 用户拒绝覆盖的尺寸（成功后清空；同尺寸不再重发） */
  let rejectedSize: { cols: number; rows: number } | null = null

  /** 待确认覆盖目标（弹窗内容源） */
  const rendererOverrideTarget = ref<{
    cols: number
    rows: number
    rendererName: string
  } | null>(null)
  const showRendererOverrideDialog = ref(false)

  /** 渲染端显示名：桌面端用 i18n 标签，移动端用设备名；source 异常时兜底，避免弹窗崩溃 */
  function rendererDisplayName(source?: RendererSource | null): string {
    if (!source || source.kind === 'desktop') return t('mobile.terminal.rendererDesktop')
    return source.deviceName || t('mobile.terminal.rendererMobile')
  }

  /** 用户确认覆盖：force 重发（尺寸移交服务端正统归属） */
  async function confirmRendererOverride() {
    const target = rendererOverrideTarget.value
    showRendererOverrideDialog.value = false
    rendererOverrideTarget.value = null
    if (target) await queueResize(target.cols, target.rows, true)
  }

  /** 用户拒绝覆盖：记录被拒尺寸，同尺寸不再打扰 */
  function cancelRendererOverride() {
    const target = rendererOverrideTarget.value
    showRendererOverrideDialog.value = false
    rendererOverrideTarget.value = null
    if (target) rejectedSize = { cols: target.cols, rows: target.rows }
  }

  /** 用户显式刷新 = 明确意图：清空「拒绝覆盖尺寸」记录，让尺寸仲裁重新走一遍 */
  function clearRejectedSize() {
    rejectedSize = null
  }

  /**
   * 尺寸请求入队（串行队列 + 合并最新值）。
   *
   * 未校准（构造期字体未就绪，cols/rows 是 80x24 兜底值）时跳过，等 fit 校准后再发：
   * 校准完成（gridCalibrated）后即便真实网格恰为 80x24 也必须下发——否则该设备的
   * PTY 永远停在会话启动时的估算尺寸（行宽不符 → 换行/定位错位）。
   */
  async function queueResize(cols: number, rows: number, force = false) {
    if (cols <= 0 || rows <= 0) return
    const sid = ctx.getSessionId()
    if (!sid || isMockSession(sid)) return
    if (!ctx.gridCalibrated.value && cols === 80 && rows === 24) return
    // 用户刚拒绝过的相同尺寸：抑制（force 重发绕过，确认覆盖是明确意图）
    if (!force && rejectedSize && rejectedSize.cols === cols && rejectedSize.rows === rows) return
    // 相同尺寸已在确认弹窗中：不再重复入队（防弹窗期间 RO 事件叠加）
    if (
      rendererOverrideTarget.value &&
      rendererOverrideTarget.value.cols === cols &&
      rendererOverrideTarget.value.rows === rows
    ) {
      return
    }

    pendingResize = { cols, rows, force }
    if (resizeInFlight) return
    resizeInFlight = true
    try {
      while (pendingResize) {
        const next = pendingResize
        pendingResize = null
        if (!ctx.isConnected()) break
        // 调试验证：记录实际发送给主机 PTY 的尺寸
        logger.debug(`[TerminalView] send resize to PTY: ${next.cols}x${next.rows}${next.force ? ' (force)' : ''}`)
        const result = await httpResizeSession(sid, next.cols, next.rows, next.force)
        if (result.code !== 0) {
          logger.warn('[TerminalView] Queue resize failed:', result.message)
          continue
        }
        const outcome = result.data
        if (!outcome) {
          // 服务端未返回裁决数据（异常响应）：按失败处理，下次触发时重试
          logger.warn('[TerminalView] Resize response missing outcome data')
          continue
        }
        if (outcome.status === 'applied') {
          // 已应用：本端即位正统渲染端（请求方即正统），清空抑制记录
          rejectedSize = null
        } else if (outcome.status === 'needsConfirmation') {
          // 另一端正渲染输出：弹窗确认是否覆盖（未应用，PTY 尺寸保持对方设置）
          rendererOverrideTarget.value = {
            cols: next.cols,
            rows: next.rows,
            rendererName: rendererDisplayName(outcome.currentCanonical),
          }
          showRendererOverrideDialog.value = true
        }
      }
    } finally {
      resizeInFlight = false
    }
  }

  /**
   * 主动同步当前终端尺寸到主机 PTY（走 queueResize 串行队列）。
   * 重连/会话激活后 PTY 重建为默认 80x24，容器尺寸未变化时 fit/onResize 都不会
   * 触发，必须显式同步一次，否则输出按错误宽度换行导致格式混乱。
   * 不依赖会话状态门控：会话列表状态可能 stale，只要 WS 已连接就同步
   * （会话不存在时服务端 404 无害）——错过同步会让 PTY 停留在桌面端宽度，
   * 移动端行尾文字被截断。
   */
  function syncTerminalSizeToHost() {
    const term = ctx.terminalRef.value
    if (!term || isMockSession(ctx.getSessionId())) return
    if (!ctx.isConnected()) return
    queueResize(term.cols, term.rows)
  }

  /**
   * resize 实际应用（防抖器 onApply 接线）：DPR 感知 fit + 仅 cols/rows 实际变化
   * 才整屏重绘 + PTY 同步（走串行队列）。subpixel 抖动（容器尺寸微调但行列不变）
   * 不触发多余重绘，避免键盘避让/旋转时每帧全量重绘的浪费。
   *
   * 此处的 refresh 与 xterm resize 内部（RenderService.handleResize → _fullRefresh）
   * 的重绘在同一帧，被 xterm 的 RenderDebouncer（rAF）合并成一次 render，零额外
   * 开销；保留它为与桌面端 applyResize 同款语义（渲染器切换/暂停恢复场景一致）。
   */
  function applyResize() {
    const term = ctx.terminalRef.value
    if (!term || !ctx.fitAddonRef.value) return
    const beforeCols = term.cols
    const beforeRows = term.rows
    ctx.callbacks.applyDprFit()
    if (term.cols !== beforeCols || term.rows !== beforeRows) {
      term.refresh(0, term.rows - 1)
      syncTerminalSizeToHost()
    }
  }

  // 注册跨域回调（调用时解析，创建顺序无关）
  ctx.callbacks.applyResize = applyResize

  return {
    rendererOverrideTarget,
    showRendererOverrideDialog,
    confirmRendererOverride,
    cancelRendererOverride,
    clearRejectedSize,
    queueResize,
    syncTerminalSizeToHost,
    applyResize,
  }
}
