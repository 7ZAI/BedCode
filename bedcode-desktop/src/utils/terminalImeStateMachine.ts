/**
 * 终端 IME 输入状态机（Linux WebKitGTK 专用去重）
 *
 * 背景：xterm.js 在 WebKitGTK（Linux Tauri webview）上输入法提交时存在
 * 「输入随机重复」bug，根因在 xterm 6.0.0 输入链路有两条数据发送路径：
 *   1. keydown keyCode=229（WebKitGTK 的输入法组合键）→ _handleAnyTextareaChanges()
 *      对 textarea 前后值求差，setTimeout 后把差值作为 onData 发出
 *   2. compositionend → _finalizeComposition(true) → 把组合起始位置到当前的
 *      子串作为 onData 再次发出
 * 两条路径发出的载荷可能完全一致（都是同一个已上屏的组合文本），且 WebKitGTK
 * 的组合事件时序不稳定（compositionstart 可能迟到/缺失、compositionend 可能
 * 提前触发），导致同一段文本被终端发送两次 → 输入栏随机重复字符。
 *
 * 解决方案：在组件层维护独立的 IME 状态机，跟踪 compositionstart/update/end
 * 与 keydown(229) 信号，对 xterm 发出的 onData 载荷做**组合窗口内精确去重**：
 * - 处于组合窗口（composing / committed 宽限期）时，同一载荷若刚发送过则丢弃
 * - 普通按键（keyCode≠229）立即复位到 idle，正常连打（如 "aa"）永不被去重
 * - 新一轮 compositionstart 清空最近载荷记录，两次独立组合同一字符不误杀
 *
 * 纯逻辑模块（Seam A）：零 DOM 依赖，事件信号由外部（TerminalPreview）转发，
 * 时间由 now() 注入（测试可传假时钟），行为完全可单测。
 */

export type ImeState = 'idle' | 'composing' | 'committed'

/** 组合提交后宽限窗口（ms）：xterm finalize 经 setTimeout(0) 发出，同 tick/次 tick 内到达 */
export const IME_COMMIT_GRACE_MS = 100
/** 最近载荷可被去重的有效时间（ms） */
export const IME_DEDUP_WINDOW_MS = 200
/** 最近载荷环形记录上限（防无限增长） */
export const IME_RECENT_MAX = 8

interface ImeRecentEntry {
  text: string
  ts: number
}

export interface ImeInputEventInfo {
  /** InputEvent.inputType（'insertText' / 'insertCompositionText' / …） */
  inputType: string
  /** InputEvent.isComposing（组合进行中为 true） */
  isComposing: boolean
  /** InputEvent.data（null 表示 delete/无数据变更） */
  data: string | null
}

export interface TerminalImeStateMachineOptions {
  /** 时钟注入（测试用假时钟；缺省 performance.now()） */
  now?: () => number
}

export class TerminalImeStateMachine {
  private state: ImeState = 'idle'
  /** 组合窗口内最近发送/观察到的载荷（环形） */
  private recent: ImeRecentEntry[] = []
  /** committed 状态截止时间（超过则回 idle） */
  private commitUntil = 0

  private readonly nowFn: () => number
  private readonly graceMs: number
  private readonly dedupMs: number

  constructor(opts: TerminalImeStateMachineOptions = {}) {
    this.nowFn = opts.now ?? (() => performance.now())
    this.graceMs = IME_COMMIT_GRACE_MS
    this.dedupMs = IME_DEDUP_WINDOW_MS
  }

  /** 当前状态（测试/诊断可见） */
  getState(): ImeState {
    return this.state
  }

  private now(): number {
    return this.nowFn()
  }

  private clearRecent(): void {
    this.recent = []
  }

  private remember(text: string): void {
    this.recent.push({ text, ts: this.now() })
    if (this.recent.length > IME_RECENT_MAX) {
      this.recent.shift()
    }
  }

  /** 组合开始：进入 composing，清空最近载荷（新会话与旧会话隔离） */
  onCompositionStart(): void {
    this.state = 'composing'
    this.clearRecent()
  }

  /** 组合更新：确认仍在组合中 */
  onCompositionUpdate(): void {
    if (this.state === 'idle') {
      // WebKitGTK 偶发在 compositionstart 前先触发 update，保守进入组合态
      this.state = 'composing'
    }
  }

  /** 组合结束：进入 committed 宽限期，等待 finalize 载荷到达 */
  onCompositionEnd(): void {
    this.state = 'committed'
    this.commitUntil = this.now() + this.graceMs
  }

  /**
   * 按键信号：keyCode 229 = 输入法组合键（WebKitGTK 可能在 compositionstart
   * 缺失时先来 229，此时保守进入组合态）；其余按键视为普通输入，复位 idle。
   */
  onKeyDown(keyCode: number): void {
    if (keyCode === 229) {
      if (this.state === 'idle') {
        this.state = 'composing'
        this.clearRecent()
      }
      return
    }
    // 普通按键：退出组合窗口，正常连打不受去重影响
    this.state = 'idle'
    this.clearRecent()
  }

  /**
   * 原生 input 事件：仅驱动状态迁移，不预记载荷。
   *
   * 为什么：xterm 的数据发送一律经 onData 出口（shouldForward 可见），
   * 而 input 事件先经 xterm 的 input handler（同元素监听按注册序先触发）
   * 才会到达本监听；若在此预记载荷，会让 shouldForward 把「首次真实发出」
   * 误判为重复。此处只负责打开组合/提交窗口，让窗口内的双发去重生效。
   */
  onInput(info: ImeInputEventInfo): void {
    if (info.isComposing) {
      this.state = 'composing'
      return
    }
    if (info.inputType === 'insertText' && info.data) {
      // WebKitGTK 组合提交可能不走 compositionend 而直接发 insertText：
      // 开提交宽限窗，防止 finalize 或后续 insertText 双发同一载荷
      this.state = 'committed'
      this.commitUntil = this.now() + this.graceMs
    }
  }

  /**
   * 去重裁决：对每条即将发送到会话的 onData 载荷判断是否放行。
   * 组合窗口内同一载荷重复出现 → false（丢弃）；否则记录并放行。
   *
   * 时序说明：xterm 的 input 监听在注册序上先于我们的监听触发，首次发出
   * 的载荷到达此处时状态可能仍是 idle，因此**始终记录**载荷；但仅活跃窗口
   * （composing / committed 宽限）参与去重。普通按键（onKeyDown 非 229）
   * 会先清空记录，故键盘连打（如 "aa"）永不被误杀。
   */
  shouldForward(data: string): boolean {
    const now = this.now()
    // committed 宽限期超时：复位 idle，正常输入不受影响
    if (this.state === 'committed' && now > this.commitUntil) {
      this.state = 'idle'
      this.clearRecent()
    }
    // 仅活跃窗口参与去重；idle 只记录不裁决（供活跃窗口的后续重复比对）
    const active = this.state !== 'idle'
    const dup =
      active &&
      this.recent.some((e) => e.text === data && now - e.ts <= this.dedupMs)
    // 始终记录（普通按键会先清空，避免连打误判；环形上限防无限增长）
    this.remember(data)
    return !dup
  }

  /** 主动复位（终端销毁/会话切换时调用） */
  reset(): void {
    this.state = 'idle'
    this.clearRecent()
    this.commitUntil = 0
  }
}