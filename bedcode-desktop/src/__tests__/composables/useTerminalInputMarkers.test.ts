/**
 * useTerminalInputMarkers 单元测试
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useTerminalInputMarkers } from '@/composables/useTerminalInputMarkers'
import type { IMarker, Terminal } from '@xterm/xterm'

/** 模拟 xterm IMarker：line 可手动调整以模拟 scrollback trim 校正 */
function createFakeMarker(initialLine = 10): IMarker & { line: number } {
  const marker = {
    id: 0,
    isDisposed: false,
    line: initialLine,
    onDispose: { event: vi.fn() } as unknown as IMarker['onDispose'],
    register: vi.fn() as unknown as IMarker['register'],
    dispose() {
      this.isDisposed = true
      this.line = -1
    },
  }
  return marker
}

/** 模拟 xterm Terminal：registerMarker 返回队列中下一个 fake marker */
function createFakeTerminal(markers: (IMarker & { line: number })[]): Terminal {
  let idx = 0
  return {
    registerMarker: vi.fn(() => markers[idx++]),
  } as unknown as Terminal
}

describe('useTerminalInputMarkers', () => {
  let markers: (IMarker & { line: number })[]
  let terminal: Terminal

  beforeEach(() => {
    markers = []
    terminal = createFakeTerminal(markers)
  })

  it('record 记录输入文本与当前行 marker', () => {
    const m1 = createFakeMarker(5)
    const m2 = createFakeMarker(8)
    markers.push(m1, m2)
    const hook = useTerminalInputMarkers()

    hook.record(terminal, 'git status')
    hook.record(terminal, 'ls -la')

    expect(hook.records.value).toHaveLength(2)
    expect(hook.records.value[0].text).toBe('git status')
    expect(hook.records.value[1].text).toBe('ls -la')
    expect(hook.visibleMarkers.value).toEqual([
      { id: 1, line: 5, text: 'git status' },
      { id: 2, line: 8, text: 'ls -la' },
    ])
  })

  it('registerMarker 返回空（alternate buffer）时不记录', () => {
    const terminalNoMarker = { registerMarker: vi.fn(() => undefined) } as unknown as Terminal
    const hook = useTerminalInputMarkers()

    hook.record(terminalNoMarker, 'vim')

    expect(hook.records.value).toHaveLength(0)
    expect(hook.visibleMarkers.value).toHaveLength(0)
  })

  it('超过 maxMarkers（默认 10）时 FIFO 淘汰最旧并 dispose', () => {
    const hook = useTerminalInputMarkers()
    for (let i = 0; i < 12; i++) {
      const m = createFakeMarker(i)
      markers.push(m)
      hook.record(terminal, `cmd-${i}`)
    }

    expect(hook.records.value).toHaveLength(10)
    // 最旧的 2 条被淘汰且已 dispose
    expect(markers[0].isDisposed).toBe(true)
    expect(markers[1].isDisposed).toBe(true)
    expect(markers[2].isDisposed).toBe(false)
    // 保留最近 10 条
    expect(hook.visibleMarkers.value[0]).toEqual({ id: 3, line: 2, text: 'cmd-2' })
    expect(hook.visibleMarkers.value[9]).toEqual({ id: 12, line: 11, text: 'cmd-11' })
  })

  it('自定义 maxMarkers 生效', () => {
    const hook = useTerminalInputMarkers({ maxMarkers: 3 })
    for (let i = 0; i < 5; i++) {
      const m = createFakeMarker(i)
      markers.push(m)
      hook.record(terminal, `cmd-${i}`)
    }

    expect(hook.records.value).toHaveLength(3)
    expect(hook.visibleMarkers.value.map((m) => m.text)).toEqual(['cmd-2', 'cmd-3', 'cmd-4'])
  })

  it('scrollback 淘汰后 marker.line < 0 被过滤（模拟 trim 校正）', () => {
    const hook = useTerminalInputMarkers()
    for (let i = 0; i < 3; i++) {
      const m = createFakeMarker(i)
      markers.push(m)
      hook.record(terminal, `cmd-${i}`)
    }

    // 模拟 scrollback trim：所有行向前移 2 行，行 0 被淘汰
    markers.forEach((m) => (m.line -= 2))

    const visible = hook.visibleMarkers.value
    expect(visible).toHaveLength(1)
    expect(visible[0]).toEqual({ id: 3, line: 0, text: 'cmd-2' })
  })

  it('全部淘汰后可见列表为空', () => {
    const hook = useTerminalInputMarkers()
    const m = createFakeMarker(3)
    markers.push(m)
    hook.record(terminal, 'cmd')

    m.line = -1

    expect(hook.visibleMarkers.value).toHaveLength(0)
  })

  it('clear 清空全部并 dispose 所有 marker', () => {
    const hook = useTerminalInputMarkers()
    for (let i = 0; i < 3; i++) {
      const m = createFakeMarker(i)
      markers.push(m)
      hook.record(terminal, `cmd-${i}`)
    }

    hook.clear()

    expect(hook.records.value).toHaveLength(0)
    expect(hook.visibleMarkers.value).toHaveLength(0)
    expect(markers.every((m) => m.isDisposed)).toBe(true)
  })
})
