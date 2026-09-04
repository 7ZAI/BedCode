/**
 * TerminalImeStateMachine 单元测试（Seam A：纯逻辑，Linux WebKitGTK IME 去重）
 *
 * 覆盖：组合开始/更新/结束状态迁移、keydown 229 双发去重、committed 宽限窗口、
 * 普通按键复位、连续相同字符（"aa"）不误杀、两次独立组合同一字符不误杀、
 * insertCompositionText / insertText 提交路径。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  TerminalImeStateMachine,
  IME_COMMIT_GRACE_MS,
  IME_DEDUP_WINDOW_MS,
} from '@/utils/terminalImeStateMachine'

/** 假时钟：测试内手动推进时间 */
function makeMachine() {
  let t = 0
  const clock = {
    now: () => t,
    advance: (ms: number) => {
      t += ms
    },
  }
  const machine = new TerminalImeStateMachine({ now: clock.now })
  return { machine, clock }
}

describe('TerminalImeStateMachine', () => {
  beforeEach(() => {})

  it('初始为 idle：直接放行所有输入', () => {
    const { machine } = makeMachine()
    expect(machine.getState()).toBe('idle')
    expect(machine.shouldForward('a')).toBe(true)
    expect(machine.shouldForward('b')).toBe(true)
  })

  it('普通按键（keyCode≠229）复位到 idle：连续相同字符不被去重', () => {
    const { machine } = makeMachine()
    machine.onKeyDown(65) // 'a'
    expect(machine.shouldForward('a')).toBe(true)
    machine.onKeyDown(65) // 再次按键 'a'
    expect(machine.shouldForward('a')).toBe(true) // 两次 'a' 都放行
  })

  it('idle 记录供活跃窗口去重：首次发出时状态 idle，重复发出时已 composing', () => {
    const { machine } = makeMachine()
    // xterm input 监听先触发：首次发出载荷时状态仍 idle（被记录）
    expect(machine.shouldForward('你')).toBe(true)
    // 随后 compositionstart 进入组合态，双发同一载荷 → 去重
    machine.onCompositionStart()
    machine.shouldForward('你') // 主动唤起一次记录（模拟：组合中再次发送）
    expect(machine.shouldForward('你')).toBe(false)
  })

  it('keydown 229 进入组合态：同一载荷重复发出被去重', () => {
    const { machine } = makeMachine()
    machine.onKeyDown(229)
    expect(machine.getState()).toBe('composing')
    // 第一次发出「你」→ 放行
    expect(machine.shouldForward('你')).toBe(true)
    // xterm 双发同一载荷（finalize 路径）→ 去重丢弃
    expect(machine.shouldForward('你')).toBe(false)
    // 不同载荷照常放行
    expect(machine.shouldForward('好')).toBe(true)
  })

  it('compositionstart → compositionend：提交宽限窗口内双发去重', () => {
    const { machine, clock } = makeMachine()
    machine.onCompositionStart()
    machine.onCompositionUpdate()
    // 组合结束：xterm finalize 经 setTimeout(0) 发出提交文本
    machine.onCompositionEnd()
    expect(machine.getState()).toBe('committed')
    expect(machine.shouldForward('你好')).toBe(true)
    // 同 tick 内重复（WebKitGTK 双发）→ 去重
    expect(machine.shouldForward('你好')).toBe(false)
    // 宽限期过后：恢复普通输入
    clock.advance(IME_COMMIT_GRACE_MS + 1)
    expect(machine.shouldForward('x')).toBe(true)
    expect(machine.getState()).toBe('idle')
  })

  it('两次独立组合输入同一字符：compositionstart 清空记录，不误杀', () => {
    const { machine } = makeMachine()
    // 第一次组合
    machine.onCompositionStart()
    machine.onCompositionEnd()
    expect(machine.shouldForward('你')).toBe(true)
    expect(machine.shouldForward('你')).toBe(false) // 双发去重

    // 第二次组合（新的 compositionstart）
    machine.onCompositionStart()
    machine.onCompositionEnd()
    expect(machine.shouldForward('你')).toBe(true) // 新会话不误杀
  })

  it('insertCompositionText（isComposing）记录中间载荷并参与去重', () => {
    const { machine } = makeMachine()
    // WebKitGTK：组合中先发 insertCompositionText（拼音中间态）
    machine.onInput({ inputType: 'insertCompositionText', isComposing: true, data: 'nihao' })
    expect(machine.getState()).toBe('composing')
    // xterm 把该中间态也通过 onData 发出 → 放行（用户可见的中间输入）
    expect(machine.shouldForward('nihao')).toBe(true)
    // 若 finalize 重复发出同一中间态 → 去重
    expect(machine.shouldForward('nihao')).toBe(false)
    // 提交后的最终文本不同 → 放行
    expect(machine.shouldForward('你好')).toBe(true)
  })

  it('insertText 提交（无 composition 事件）开宽限窗并去重', () => {
    const { machine } = makeMachine()
    // WebKitGTK 某些输入法：提交时不发 compositionend，直接 insertText
    machine.onInput({ inputType: 'insertText', isComposing: false, data: '你' })
    expect(machine.getState()).toBe('committed')
    // 首次 onData 发出该文本 → 放行
    expect(machine.shouldForward('你')).toBe(true)
    // finalize 或后续 insertText 双发同一文本 → 去重
    expect(machine.shouldForward('你')).toBe(false)
  })

  it('dedup 窗口过期后相同文本可再次放行', () => {
    const { machine, clock } = makeMachine()
    machine.onKeyDown(229)
    expect(machine.shouldForward('你')).toBe(true)
    clock.advance(IME_DEDUP_WINDOW_MS + 1)
    // 超过 dedup 时效：新输入相同文本不再被误判
    expect(machine.shouldForward('你')).toBe(true)
  })

  it('reset 复位到 idle 并清空记录', () => {
    const { machine } = makeMachine()
    machine.onCompositionStart()
    machine.shouldForward('你')
    machine.reset()
    expect(machine.getState()).toBe('idle')
    expect(machine.shouldForward('你')).toBe(true)
  })

  it('普通按键发生在组合前：先按键后组合，记录互不干扰', () => {
    const { machine } = makeMachine()
    machine.onKeyDown(13) // Enter
    expect(machine.getState()).toBe('idle')
    machine.onCompositionStart()
    expect(machine.getState()).toBe('composing')
    expect(machine.shouldForward('你')).toBe(true)
  })
})