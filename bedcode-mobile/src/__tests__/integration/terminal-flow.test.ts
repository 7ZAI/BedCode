/**
 * 终端流组合集成测试（store × useTerminalBuffer × writeCoalescer × xterm × 输入回传）
 *
 * 协作实体：terminalBuffer store（Rust 命令驱动 + terminal-* 事件消费 + 字节游标） +
 * useTerminalBuffer（实时 handler 注册 / 历史拼接 writeParsed）+ writeCoalescer（rAF 合并写入） +
 * xterm Terminal（stub，遵循 writeCoalescer.test.ts 的项目惯例） + useMobileCommands（真实模块，
 * 其 invoke 走 mock 的 @tauri-apps/api/core）。
 *
 * 测试 seam：mock Tauri invoke（terminal_get_history / terminal_send_input 等命令面）+ mockListen
 * 捕获 terminal-frame / terminal-state 事件并手动驱动——模拟移动端 Rust 后端（terminal_link.rs）
 * 的帧/状态推送。
 *
 * 覆盖：terminal-frame 事件 → writeCoalescer → terminal.write 全链路 + lastRenderedOffset 推进；
 * 历史拼接期间实时帧缓冲 → 历史写完 FLUSH（快照拼接无缝隙）；输入回传（sendInput →
 * terminal_send_input 命令，不再走旧 HTTP POST / WS socket）。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { Terminal } from '@xterm/xterm'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { flushAsync } from './helpers'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const mockListen = vi.fn((event: string, handler: (payload: unknown) => void) => {
  if (!eventHandlers[event]) eventHandlers[event] = []
  eventHandlers[event].push(handler)
  return Promise.resolve(() => {
    eventHandlers[event] = (eventHandlers[event] || []).filter((h) => h !== handler)
  })
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => p,
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
  emit: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('vue-sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn(), info: vi.fn(), message: vi.fn() },
}))
vi.mock('@tauri-apps/plugin-os', () => ({}))

// ==================== 测试基建 ====================

/** stub xterm（项目惯例：writeCoalescer.test.ts 同款；write 回调驱动 writeParsed 背压） */
function makeMockTerminal() {
  return {
    element: {},
    write: vi.fn((_data: unknown, cb?: () => void) => cb?.()),
    clear: vi.fn(),
    // 渲染背压接线（registerRealtimeHandler 挂 onWriteParsed）
    onWriteParsed: vi.fn(() => ({ dispose: vi.fn() })),
  } as unknown as Terminal
}

/** base64 编码（构造 terminal-frame 事件载荷） */
function b64(text: string): string {
  return btoa(unescape(encodeURIComponent(text))).replace(/\+/g, '-').replace(/\//g, '_')
}

/** 模拟移动端 Rust 推送实时帧事件（与 terminal_link.rs emit 键一致） */
function emitFrame(sessionId: string, start: number, end: number, data: string) {
  for (const h of eventHandlers['terminal-frame'] ?? []) {
    h({ payload: { session_id: sessionId, start_offset: start, end_offset: end, data_base64: b64(data) } })
  }
}

/** 模拟移动端 Rust 链路状态事件 */
function emitState(sessionId: string, phase: string, detail?: string) {
  for (const h of eventHandlers['terminal-state'] ?? []) {
    h({ payload: { session_id: sessionId, phase, detail } })
  }
}

/** 汇总 xterm.write 收到的全部字节（rAF 合并可能把同帧事件并成一次 write） */
function writtenText(terminal: Terminal): string {
  const sink = terminal as unknown as { write: ReturnType<typeof vi.fn> }
  return sink.write.mock.calls.map(([d]) => new TextDecoder().decode(d as Uint8Array)).join('')
}

function installInvokeMock() {
  // 默认：terminal_get_history 空历史；其余命令 no-op
  mockInvoke.mockImplementation((cmd: string, args: any) => {
    if (cmd === 'terminal_get_history') {
      return Promise.resolve({
        from: args?.from ?? 0,
        minOffset: 0,
        snapshotOffset: 0,
        historyBytes: 0,
        dataBase64: '',
      })
    }
    return Promise.resolve(undefined)
  })
}

beforeEach(async () => {
  vi.clearAllMocks()
  installInvokeMock()
  setActivePinia(createPinia())
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

describe('终端流：terminalBuffer store × useTerminalBuffer × xterm × 输入回传', () => {
  it('实时帧全链路：terminal-frame 事件 → writeCoalescer → terminal.write + 游标推进', async () => {
    const store = useTerminalBufferStore()
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    // 等历史拼接（空历史）完成——此后实时帧直接写入
    await flushAsync()

    emitFrame('s1', 0, 5, 'hello')
    emitFrame('s1', 5, 10, 'world')
    await flushAsync()

    expect(writtenText(terminal)).toBe('helloworld')
    // 字节游标推进（lastRenderedOffset = 已渲染帧末）
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(10)
  })

  it('历史拼接期间实时帧缓冲 → 历史写完 FLUSH（快照拼接无缝隙）', async () => {
    // 挂起 terminal_get_history：控制历史拼接时序
    let resolveHistory!: (v: unknown) => void
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'terminal_get_history') {
        return new Promise((resolve) => {
          resolveHistory = resolve
        })
      }
      return Promise.resolve(undefined)
    })

    const store = useTerminalBufferStore()
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)
    await flushAsync(1)

    // 拼接期间实时帧到达（start ≥ snapshot）：缓冲不落 xterm
    emitFrame('s1', 10, 15, 'live')
    await flushAsync()
    expect(terminal.write).not.toHaveBeenCalled()

    // 历史返回：先写历史段再 FLUSH 实时缓冲——顺序无缝隙
    resolveHistory({
      from: 0,
      minOffset: 0,
      snapshotOffset: 10,
      historyBytes: 10,
      dataBase64: b64('his'),
    })
    await flushAsync()

    expect(writtenText(terminal)).toBe('hislive')
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(15)
  })

  it('输入回传：sendInput → terminal_send_input 命令（Rust → WS → 桌面 PTY）', async () => {
    const store = useTerminalBufferStore()

    // 订阅 + 链路 live
    await store.subscribeSession('s1')
    emitState('s1', 'live')
    await flushAsync()

    const ok = store.sendInput('s1', 'ls -la\n', 'enter')
    expect(ok).toBe(true)
    await flushAsync()
    expect(invokeCalls('terminal_send_input')).toContainEqual([
      { sessionId: 's1', data: 'ls -la\n', specialKey: 'enter' },
    ])
    // 旧链路不再被使用：无 HTTP POST / WS socket 通道
    expect(invokeCalls('http_request')).toHaveLength(0)

    // 未订阅会话输入被拒（不发送）
    const rejected = store.sendInput('s2', 'x')
    expect(rejected).toBe(false)
    expect(invokeCalls('terminal_send_input')).toHaveLength(1)
  })
})

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}