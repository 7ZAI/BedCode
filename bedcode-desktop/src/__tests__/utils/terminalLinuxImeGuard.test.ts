/**
 * Linux IME 防护接线回归测试（真实 xterm + happy-dom）
 *
 * 协作实体：真实 xterm Terminal（open() 进带尺寸 DOM 元素）+ 真实
 * attachLinuxImeGuard（含 TerminalImeStateMachine 与 textarea 清空），
 * 仅以「手动派发 composition/input/keydown 事件 + 手动设置 textarea.value」
 * 模拟浏览器（WebKitGTK）的 IME 事件序列——真实浏览器里预编辑/提交文本由
 * 引擎写入 textarea，测试中在对应事件时机同步赋值等价模拟。
 *
 * 覆盖症状：中文输入法按空格提交后「随机重复之前输入的字符」
 * （WebKitGTK 偶发丢失 compositionstart → xterm _compositionPosition.start
 * 停留旧值 → finalize 发出 旧文本 + 新文本 的拼接载荷）。
 *
 * 约束：真实 timers（xterm finalize 与防护清空均为 setTimeout(0)，依赖 FIFO
 * 顺序，不用 fake timers）。
 */
import { describe, it, expect, afterEach } from 'vitest'
import { Terminal } from '@xterm/xterm'
import { attachLinuxImeGuard, type LinuxImeGuard } from '@/utils/terminalLinuxImeGuard'

// ==================== 测试基建 ====================

let term: Terminal | null = null
let host: HTMLElement | null = null

/** 创建已 open 并挂好 IME 防护的终端；返回 onData 出口放行后的载荷记录 */
function createGuardedTerminal(): {
  guard: LinuxImeGuard
  sent: string[]
  ta: HTMLTextAreaElement
} {
  host = document.createElement('div')
  host.style.width = '800px'
  host.style.height = '400px'
  document.body.appendChild(host)
  term = new Terminal({ cols: 80, rows: 24 })
  term.open(host)
  const guard = attachLinuxImeGuard(term)
  expect(guard).not.toBeNull()
  const sent: string[] = []
  term.onData((data) => {
    if (guard!.shouldForward(data)) sent.push(data)
  })
  return { guard: guard!, sent, ta: term.textarea! }
}

/** 派发组合事件（xterm 的 start/end 处理不读事件属性，普通 Event 足够） */
function dispatchComposition(ta: HTMLTextAreaElement, type: string): void {
  ta.dispatchEvent(new Event(type, { bubbles: true }))
}

/** 派发 input 事件（inputType/isComposing/data 用 defineProperty 兜底环境差异） */
function dispatchInput(
  ta: HTMLTextAreaElement,
  opts: { inputType: string; isComposing: boolean; data: string | null },
): void {
  const ev = new InputEvent('input', { bubbles: true })
  Object.defineProperty(ev, 'inputType', { value: opts.inputType })
  Object.defineProperty(ev, 'isComposing', { value: opts.isComposing })
  Object.defineProperty(ev, 'data', { value: opts.data })
  ta.dispatchEvent(ev)
}

/** 派发 keydown（keyCode 不在标准 init 字典内，defineProperty 注入） */
function dispatchKeydown(ta: HTMLTextAreaElement, keyCode: number): void {
  const ev = new KeyboardEvent('keydown', { bubbles: true, cancelable: true })
  Object.defineProperty(ev, 'keyCode', { value: keyCode })
  ta.dispatchEvent(ev)
}

/** 等待 xterm finalize 的 setTimeout(0) 与防护的清空 setTimeout(0) 均执行完 */
async function flushAsync(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
}

afterEach(() => {
  term?.dispose()
  term = null
  host?.remove()
  host = null
})

// ==================== 场景 ====================

describe('attachLinuxImeGuard：WebKitGTK IME 防护', () => {
  it('丢失 compositionstart 的组合不再重发上一轮已提交文本（拼接型重复根因）', async () => {
    const { sent, ta } = createGuardedTerminal()

    // 第一轮：正常组合（输入「你好」，空格提交）
    dispatchComposition(ta, 'compositionstart')
    ta.value = '你好'
    dispatchComposition(ta, 'compositionend')
    await flushAsync()

    // finalize 发出本轮组合文本，且防护已在其后清空 textarea
    expect(sent).toEqual(['你好'])
    expect(ta.value).toBe('')

    // 第二轮：WebKitGTK 偶发丢失 compositionstart（无 start 事件，直接提交）
    ta.value = '世界'
    dispatchComposition(ta, 'compositionend')
    await flushAsync()

    // 旧行为：start 停留旧值 0，finalize 发出 substring(0) = '你好世界'
    // （重复上一轮「你好」）。修复后 textarea 已清空，substring(0) 恰为本轮文本
    expect(sent).toEqual(['你好', '世界'])
    expect(ta.value).toBe('')
  })

  it('非组合 insertText 提交后清空 textarea，下轮丢 compositionstart 不再拼接重发', async () => {
    const { sent, ta } = createGuardedTerminal()

    // IME 候选数字/符号直接上屏路径：keydown(229) → input(insertText, 非组合)
    dispatchKeydown(ta, 229)
    ta.value = '5'
    dispatchInput(ta, { inputType: 'insertText', isComposing: false, data: '5' })
    await flushAsync()

    // xterm _inputEvent 路径发送一次；防护同步清空，'5' 不残留 textarea
    expect(sent).toEqual(['5'])
    expect(ta.value).toBe('')

    // 随后一轮丢失 compositionstart 的组合：只发本轮文本（旧实现会发出 '5你好'）
    ta.value = '你好'
    dispatchComposition(ta, 'compositionend')
    await flushAsync()
    expect(sent).toEqual(['5', '你好'])
  })

  it('compositionend 与 insertText 双路径竞态：同一提交文本只发一次', async () => {
    const { sent, ta } = createGuardedTerminal()

    dispatchComposition(ta, 'compositionstart')
    ta.value = '你好'
    // WebKitGTK 提交序：finalize（compositionend 路径）与 insertText 各发一次
    dispatchComposition(ta, 'compositionend')
    dispatchInput(ta, { inputType: 'insertText', isComposing: false, data: '你好' })
    await flushAsync()

    // insertText 先经 xterm _inputEvent 发出；finalize 的 setTimeout 读到的
    // textarea 已被 insertText 后的同步清空置空，不再补发；即使事件顺序颠倒，
    // 状态机精确去重兜底——最终恰好一次
    expect(sent).toEqual(['你好'])
  })

  it('清空定时器触发前新组合已开始：跳过清空，不破坏进行中的预编辑', async () => {
    const { sent, ta } = createGuardedTerminal()

    // 第一轮提交
    dispatchComposition(ta, 'compositionstart')
    ta.value = '你好'
    dispatchComposition(ta, 'compositionend')
    // 同一任务内立即开始第二轮（清空定时器尚未触发）
    dispatchComposition(ta, 'compositionstart')
    ta.value = '你好世'
    await flushAsync()

    // 清空被跳过：预编辑中间态保留（IME 状态未被打断）
    expect(ta.value).toBe('你好世')

    // 第二轮提交：compositionstart 已把起点锚定到 value.length，只发本轮增量
    ta.value = '你好世界'
    dispatchComposition(ta, 'compositionend')
    await flushAsync()
    expect(sent).toEqual(['你好', '世界'])
    expect(ta.value).toBe('')
  })

  it('两次独立组合输入同一文本：不误杀（新组合清空最近载荷记录）', async () => {
    const { sent, ta } = createGuardedTerminal()

    for (const text of ['好', '好']) {
      dispatchComposition(ta, 'compositionstart')
      ta.value = text
      dispatchComposition(ta, 'compositionend')
      await flushAsync()
      expect(ta.value).toBe('')
    }

    expect(sent).toEqual(['好', '好'])
  })

  it('dispose 拆除监听：后续组合事件不再触发清空与去重裁决状态迁移', async () => {
    const { guard, sent, ta } = createGuardedTerminal()

    dispatchComposition(ta, 'compositionstart')
    ta.value = '你好'
    dispatchComposition(ta, 'compositionend')
    await flushAsync()
    expect(sent).toEqual(['你好'])
    expect(ta.value).toBe('')

    guard.dispose()

    // dispose 后的提交不再被清空（textarea 保留），去重裁决退化为恒放行之外的
    // 纯记录路径——仅验证监听已拆除、无异常抛出
    dispatchComposition(ta, 'compositionstart')
    ta.value = '再见'
    dispatchComposition(ta, 'compositionend')
    await flushAsync()
    expect(ta.value).toBe('再见')
  })
})
