/**
 * Linux WebKitGTK 终端 IME 防护接线（TerminalPreview 专用聚合入口）
 *
 * xterm 6.0.0 在 WebKitGTK（Linux Tauri webview）上输入法提交存在多层「随机重复
 * 输入」问题，本模块把三层缓解聚合为单一 attach 点（仅 Linux 调用；Windows/macOS
 * 走 xterm 原生路径不变）：
 *
 * 1. 关闭 keydown(229) 遗留差值路径：_handleAnyTextareaChanges 在「未组合但收到
 *    229」时对 textarea 前后值求差补发（setTimeout(0)），与 input 路径竞态会把已
 *    上屏文本的后缀再发一次（输入 "bug" 被补发 "ug"）；差值恒为 textarea 值的
 *    后缀，不是完整历史载荷，精确去重覆盖不了，故从源头置空。
 * 2. 组合窗口内精确载荷去重（TerminalImeStateMachine）：compositionend 的
 *    _finalizeComposition 与 _inputEvent(insertText) 两条路径竞态时会把同一提交
 *    文本各发一次，shouldForward 在 onData 出口丢弃重复项。
 * 3. textarea 提交后清空（拼接型重复的根因修复）：xterm 组合提交后从不清空
 *    textarea（仅 blur / Ctrl+C / Enter 清空），已提交文本跨组合无限累积；而
 *    _compositionPosition.start 只在 compositionstart 里更新——WebKitGTK 偶发
 *    丢失/延迟 compositionstart 时，start 停在上一轮组合起点，compositionend 的
 *    _finalizeComposition 发出 value.substring(旧起点) = 上一轮已提交文本 + 本轮
 *    文本，表现为「按空格提交中文后随机重复之前输入的字符」。拼接型载荷不是任何
 *    单条历史载荷的精确重复，第 2 层无法覆盖。清空后维持「value 只含当前组合文本、
 *    起点恒为 0」的不变量：即使 compositionstart 丢失，substring(0) 恰为本轮组合
 *    文本；即使提交走 insertText 而非 compositionend，同步清空同样消除残留。
 *
 * 事件顺序依据：xterm 的 textarea 监听在 open() 时先于本模块注册，且其 cancel()
 * 仅 stopPropagation（无 stopImmediatePropagation），同元素监听按注册序全部触发
 * ——本模块的监听恒在 xterm 处理完同一事件后运行，读取/清空不与 xterm 抢跑。
 */
import type { Terminal } from '@xterm/xterm'
import { TerminalImeStateMachine } from './terminalImeStateMachine'

/** xterm 未公开的内部结构（CompositionHelper 挂在 _core 上，公开 API 不暴露） */
interface XtermInternals {
  _core?: {
    _compositionHelper?: {
      _handleAnyTextareaChanges?: () => void
    }
  }
}

/** attachLinuxImeGuard 返回的防护句柄 */
export interface LinuxImeGuard {
  /** onData 出口去重裁决：组合窗口内同一载荷重复出现时返回 false（应丢弃） */
  shouldForward(data: string): boolean
  /** 状态复位（会话切换等场景） */
  reset(): void
  /** 拆除监听与未决清空定时器（终端销毁时调用；textarea 随 xterm 一并销毁） */
  dispose(): void
}

/**
 * 为已 open() 的 xterm 实例挂接 Linux WebKitGTK IME 防护。
 *
 * @param term 已完成 open() 的终端（textarea 已创建）
 * @returns 防护句柄；textarea 缺失（open 未完成/异常）返回 null，调用方跳过防护
 */
export function attachLinuxImeGuard(term: Terminal): LinuxImeGuard | null {
  const ta = term.textarea
  if (!ta) return null

  const sm = new TerminalImeStateMachine()

  // 缓解 1：关闭 keydown(229) 差值补发路径（见文件头第 1 条）
  const core = (term as unknown as XtermInternals)._core
  if (core?._compositionHelper?._handleAnyTextareaChanges) {
    core._compositionHelper._handleAnyTextareaChanges = () => {}
  }

  // 缓解 3：组合提交后排一个清空定时器。xterm finalize 在其 compositionend
  // 监听（注册序在前）里调度同为 0ms 的 setTimeout 读 textarea.value，本定时器
  // 排队在后、FIFO 晚于其执行，不会截断 finalize 的读取。
  let clearTimer: ReturnType<typeof setTimeout> | null = null
  const scheduleClear = (): void => {
    if (clearTimer !== null) clearTimeout(clearTimer)
    clearTimer = setTimeout(() => {
      clearTimer = null
      // 新一轮组合已开始（快速连打时 compositionstart 可先于本定时器触发）：
      // 跳过清空避免清掉进行中的预编辑文本。残留文本不影响正确性——该轮
      // compositionstart 已把起点锚定到 value.length，仅推迟清空时机。
      if (sm.getState() === 'composing') return
      ta.value = ''
    }, 0)
  }

  const onCompositionStart = (): void => sm.onCompositionStart()
  const onCompositionUpdate = (): void => sm.onCompositionUpdate()
  const onCompositionEnd = (): void => {
    sm.onCompositionEnd()
    scheduleClear()
  }
  const onKeyDown = (e: KeyboardEvent): void => sm.onKeyDown(e.keyCode)
  const onInput = (e: Event): void => {
    const ie = e as InputEvent
    sm.onInput({
      inputType: ie.inputType || '',
      isComposing: ie.isComposing ?? false,
      data: ie.data,
    })
    // 非组合 insertText 提交（IME 候选数字/符号直接上屏，不经 compositionend）：
    // xterm _inputEvent 读的是事件 data 且其监听先执行，此处同步清空安全；
    // 不清空则该文本残留 textarea，下轮丢失 compositionstart 时会被拼接重发
    if (!ie.isComposing && ie.inputType === 'insertText' && ie.data) {
      ta.value = ''
    }
  }

  ta.addEventListener('compositionstart', onCompositionStart)
  ta.addEventListener('compositionupdate', onCompositionUpdate)
  ta.addEventListener('compositionend', onCompositionEnd)
  ta.addEventListener('keydown', onKeyDown)
  ta.addEventListener('input', onInput)

  return {
    shouldForward: (data: string) => sm.shouldForward(data),
    reset: () => sm.reset(),
    dispose: () => {
      if (clearTimer !== null) {
        clearTimeout(clearTimer)
        clearTimer = null
      }
      ta.removeEventListener('compositionstart', onCompositionStart)
      ta.removeEventListener('compositionupdate', onCompositionUpdate)
      ta.removeEventListener('compositionend', onCompositionEnd)
      ta.removeEventListener('keydown', onKeyDown)
      ta.removeEventListener('input', onInput)
      sm.reset()
    },
  }
}
