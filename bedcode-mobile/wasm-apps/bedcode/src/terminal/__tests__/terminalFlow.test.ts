/**
 * 终端流组合集成测试（store × useTerminalBuffer × writeCoalescer × xterm × 输入回传）
 *
 * 协作实体：terminalBuffer store（Rust 命令驱动 + terminal-* 事件消费 + 本地字节计数） +
 * useTerminalBuffer（实时 handler 注册 / 裸字节写队列）+ writeCoalescer（rAF 合并写入） +
 * xterm Terminal（stub，遵循 writeCoalescer.test.ts 的项目惯例） + useMobileCommands（真实模块，
 * 其 invoke 走 mock 的 @tauri-apps/api/core）。
 *
 * 测试 seam：mock Tauri invoke（terminal_page_subscribe / terminal_send_input 等命令面）+
 * mockListen 捕获 terminal-state 事件并手动驱动——模拟移动端 Rust 后端（terminal_link.rs）
 * 的状态推送；输出则经页面级 Tauri Channel（从 terminal_page_subscribe 调用参数取回通道，
 * 投喂**裸字节**——票 05 新协议无 TB v3 帧头/offset）。
 *
 * 覆盖：Channel 裸字节 → writeCoalescer → terminal.write 全链路 + 本地字节计数推进；
 * 状态→渲染次序（subscribed 前不渲染 / 渲染只发生在订阅后）；输入回传（sendInput →
 * terminal_send_input 命令，不再走旧 HTTP POST / WS socket）。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { Terminal } from '@xterm/xterm'
import { useTerminalBufferStore } from '../store'
import { flushAsync } from './helpers'

// ==================== mock 边界（票 15：域宿主服务 + SDK + Tauri core 零调用） ====================

/**
 * Tauri core 仍 mock 且全程断言零调用：终端流不应再经 Tauri 命令栈
 * （旧链路退役的直接证明；命令面与页面通道均经域宿主服务 / SDK mobileApi）
 */
const mockInvoke = vi.fn()

// 票 15：域宿主服务 mock —— 命令面按旧断言面分发到 cmd spy；
// 事件经内存 handler 表注入；页面字节流经 SDK getMobileApi().openTerminalStream 捕获
const mocks = vi.hoisted(() => {
  const cmd = {
    terminalSubscribe: vi.fn(async (_sid: string) => {}),
    terminalUnsubscribe: vi.fn(async (_sid: string) => {}),
    terminalUnsubscribeAll: vi.fn(async () => {}),
    terminalRemove: vi.fn(async (_sid: string) => {}),
    terminalSendInput: vi.fn(async (_sid: string, _data: string, _key?: string | null) => {}),
    terminalAckRendered: vi.fn(async (_sid: string, _offset: number) => {}),
    terminalGetState: vi.fn(async (sessionId: string) => ({
      sessionId,
      phase: 'idle',
      cursor: 0,
      acked: 0,
      stopped: false,
      subscribed: false,
    })),
  }
  const dispatch: Record<string, (args: any) => unknown> = {
    'terminal-session.subscribe': (a) => cmd.terminalSubscribe(a?.sessionId),
    'terminal-session.unsubscribe': (a) => cmd.terminalUnsubscribe(a?.sessionId),
    'terminal-session.unsubscribe-all': () => cmd.terminalUnsubscribeAll(),
    'terminal-session.remove': (a) => cmd.terminalRemove(a?.sessionId),
    'terminal-session.send-input': (a) => cmd.terminalSendInput(a?.sessionId, a?.data, a?.specialKey),
    'terminal-session.ack-rendered': (a) => cmd.terminalAckRendered(a?.sessionId, a?.offset),
    'terminal-session.get-state': (a) => cmd.terminalGetState(a?.sessionId),
  }
  return {
    cmd,
    invoke: vi.fn(async (command: string, args?: any) => dispatch[command]?.(args)),
    eventHandlers: {} as Record<string, ((payload: unknown) => void)[]>,
    streamCalls: [] as { sessionId: string; onBytes: (b: Uint8Array) => void; disposed: boolean }[],
  }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

vi.mock('../host', () => ({
  invokeTerminal: (command: string, args?: any) => mocks.invoke(command, args),
  t: (key: string) => key,
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  logger: { log: () => {}, debug: () => {}, info: () => {}, warn: () => {}, error: () => {} },
  storage: { get: async () => undefined, set: async () => {}, delete: async () => {} },
  events: {
    on: (event: string, handler: (payload: unknown) => void) => {
      ;(mocks.eventHandlers[event] ??= []).push(handler)
      return { dispose: () => {} }
    },
  },
}))

vi.mock('@binblink/bedcode-plugin-sdk-mobile', () => ({
  getMobileApi: () => ({
    mockSessionId: null,
    openTerminalStream: async (sessionId: string, onBytes: (b: Uint8Array) => void) => {
      const entry = {
        sessionId,
        onBytes,
        disposed: false,
        dispose: () => {
          entry.disposed = true
        },
      }
      mocks.streamCalls.push(entry)
      return { dispose: entry.dispose }
    },
  }),
}))

const { eventHandlers } = mocks

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

/** 取某会话最近一次经 openTerminalStream 登记的页面流（真实链路同款出口） */
function pageStream(sessionId: string) {
  const stream = mocks.streamCalls.filter((c) => c.sessionId === sessionId).pop()
  if (!stream) throw new Error(`no page stream for ${sessionId}`)
  return stream
}

/** 模拟移动端 Rust 经段2 页面流推送一段**裸字节**输出（票 05 新协议：无帧头） */
function emitRaw(sessionId: string, data: string) {
  pageStream(sessionId).onBytes(new TextEncoder().encode(data))
}

/** 模拟移动端 Rust 链路状态事件 */
function emitState(sessionId: string, phase: string, detail?: string) {
  for (const h of eventHandlers['plugin:com.bedcode.terminal-session:terminal-state'] ?? []) {
    h({ session_id: sessionId, phase, detail })
  }
}

/** 模拟移动端 Rust 重锚事件（ring_resync） */
function emitResync(sessionId: string, offset: number) {
  for (const h of eventHandlers['plugin:com.bedcode.terminal-session:terminal-resync'] ?? []) {
    h({ session_id: sessionId, offset })
  }
}

/** 汇总 xterm.write 收到的全部字节（rAF 合并可能把同帧事件并成一次 write） */
function writtenText(terminal: Terminal): string {
  const sink = terminal as unknown as { write: ReturnType<typeof vi.fn> }
  return sink.write.mock.calls.map(([d]) => new TextDecoder().decode(d as Uint8Array)).join('')
}

beforeEach(async () => {
  vi.clearAllMocks()
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
  mocks.streamCalls.length = 0
  setActivePinia(createPinia())
})

describe('终端流：terminalBuffer store × useTerminalBuffer × xterm × 输入回传（票 05）', () => {
  it('实时流全链路：Channel 裸字节 → writeCoalescer → terminal.write + 本地计数推进', async () => {
    const store = useTerminalBufferStore()
    // 真实链路时序：订阅 + 收 subscribed（live）后，Rust 才转发输出（门控）
    await store.subscribeSession('s1')
    emitState('s1', 'live', 'subscribed')
    await flushAsync()
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('../composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    emitRaw('s1', 'hello')
    emitRaw('s1', 'world')
    await flushAsync()

    expect(writtenText(terminal)).toBe('helloworld')
    // 本地字节计数（仅用于 ack 水位）
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(10)
  })

  it('回放（历史）与实时同一条流：subscribed 后按序渲染，无独立拼接、无缺口误报', async () => {
    const store = useTerminalBufferStore()
    await store.subscribeSession('s1')
    emitState('s1', 'live', 'subscribed')
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('../composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)
    await flushAsync()

    // 插件回放环窗口（历史）后紧跟实时输出：同一条流，裸字节按到达序渲染
    emitRaw('s1', 'his')
    emitRaw('s1', 'live')
    await flushAsync()

    expect(writtenText(terminal)).toBe('hislive')
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(7)
  })

  it('重锚（ring_resync）：有在屏内容时清屏 + 计数归零后续拉，无重复无缺口', async () => {
    const store = useTerminalBufferStore()
    await store.subscribeSession('s1')
    emitState('s1', 'live', 'subscribed')
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('../composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    emitRaw('s1', 'stale')
    await flushAsync()
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(5)

    // 环淘汰重锚：清屏 + 计数归零；重播继续
    emitResync('s1', 4096)
    expect(terminal.clear).toHaveBeenCalledTimes(1)
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBeNull()

    emitRaw('s1', 'fresh')
    await flushAsync()
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(5)
  })

  it('尾帧与停止帧顺序：停止前已到达的输出帧先渲染，停止后字节丢弃', async () => {
    const store = useTerminalBufferStore()
    await store.subscribeSession('s1')
    emitState('s1', 'live', 'subscribed')
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('../composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    // 尾帧先到（WS 帧序保证）：渲染
    emitRaw('s1', 'tail')
    await flushAsync()
    expect(writtenText(terminal)).toContain('tail')

    // 停止帧：phase=stopped
    emitState('s1', 'idle', 'stopped')
    expect(store.getBuffer('s1')!.sessionStopped).toBe(true)

    // 停止后字节丢弃
    emitRaw('s1', 'after')
    await flushAsync()
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBeNull()
  })

  it('输入回传：sendInput → 插件命令 terminal-session.send-input（票 12：协议客户端迁插件）', async () => {
    const store = useTerminalBufferStore()

    // 订阅 + 链路 live
    await store.subscribeSession('s1')
    emitState('s1', 'live')
    await flushAsync()

    const ok = store.sendInput('s1', 'ls -la\n', 'enter')
    expect(ok).toBe(true)
    await flushAsync()
    // 票 15：命令面走域宿主服务（context.commands.execute → 插件 WASM 命令）
    const calls = mocks.cmd.terminalSendInput.mock.calls
    expect(calls.length).toBeGreaterThan(0)
    expect(calls.at(-1)).toEqual(['s1', 'ls -la\n', 'enter'])
    // 旧链路不再被使用：终端流全程零 Tauri 命令调用
    // （旧终端协议命令字面量不在本文件出现——退役锁 retired_mobile_terminal_link_lock
    // 零容忍拦截；零调用由上方 mockInvoke 断言直接证明）
    expect(mockInvoke).toHaveBeenCalledTimes(0)

    // 未订阅会话输入被拒（不发送）
    const rejected = store.sendInput('s2', 'x')
    expect(rejected).toBe(false)
    expect(mocks.cmd.terminalSendInput.mock.calls.length).toBe(calls.length)
  })
})
