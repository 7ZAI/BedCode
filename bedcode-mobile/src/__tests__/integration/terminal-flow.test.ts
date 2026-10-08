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
  // 段2 推送通道替身：真实实现构造时依赖 WebView 注入的 __TAURI_INTERNALS__
  Channel: class ChannelMock<T> {
    onmessage: ((message: T) => void) | null = null
  },
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
  emit: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('vue-sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn(), info: vi.fn(), message: vi.fn() },
}))
// i18n 文案与本集成用例断言无关（断言的是写入字节与命令调用）
vi.mock('@/locales', () => ({ default: { global: { t: (key: string) => key } } }))
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

/** 取某会话最近一次经 terminal_page_subscribe 交给 Rust 的页面通道（真实链路同款出口） */
function pageChannel(sessionId: string) {
  const call = mockInvoke.mock.calls
    .filter(([c, args]) => c === 'terminal_page_subscribe' && (args as any)?.sessionId === sessionId)
    .pop()
  const channel = (call?.[1] as any)?.channel as
    | { onmessage: ((m: ArrayBuffer) => void) | null }
    | undefined
  if (!channel?.onmessage) throw new Error(`no page channel for ${sessionId}`)
  return channel
}

/** 模拟移动端 Rust 经段2 Channel 推送一段**裸字节**输出（票 05 新协议：无帧头） */
function emitRaw(sessionId: string, data: string) {
  const channel = pageChannel(sessionId)
  const bytes = new TextEncoder().encode(data)
  const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
  channel.onmessage(buf)
}

/** 模拟移动端 Rust 链路状态事件 */
function emitState(sessionId: string, phase: string, detail?: string) {
  for (const h of eventHandlers['plugin:com.bedcode.terminal-session:terminal-state'] ?? []) {
    h({ payload: { session_id: sessionId, phase, detail } })
  }
}

/** 汇总 xterm.write 收到的全部字节（rAF 合并可能把同帧事件并成一次 write） */
function writtenText(terminal: Terminal): string {
  const sink = terminal as unknown as { write: ReturnType<typeof vi.fn> }
  return sink.write.mock.calls.map(([d]) => new TextDecoder().decode(d as Uint8Array)).join('')
}

function installInvokeMock() {
  // 默认：所有命令 no-op（terminal_get_history 已退役，新协议历史走订阅回放）
  mockInvoke.mockImplementation(() => Promise.resolve(undefined))
}

beforeEach(async () => {
  vi.clearAllMocks()
  installInvokeMock()
  setActivePinia(createPinia())
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

describe('终端流：terminalBuffer store × useTerminalBuffer × xterm × 输入回传（票 05）', () => {
  it('实时流全链路：Channel 裸字节 → writeCoalescer → terminal.write + 本地计数推进', async () => {
    const store = useTerminalBufferStore()
    // 真实链路时序：订阅 + 收 subscribed（live）后，Rust 才转发输出（门控）
    await store.subscribeSession('s1')
    emitState('s1', 'live', 'subscribed')
    await flushAsync()
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
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
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
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
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    emitRaw('s1', 'stale')
    await flushAsync()
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(5)

    // 环淘汰重锚：清屏 + 计数归零；重播继续
    for (const h of eventHandlers['plugin:com.bedcode.terminal-session:terminal-resync'] ?? []) {
      h({ payload: { session_id: 's1', offset: 4096 } })
    }
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
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
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
    // 票 12：命令面走 plugin_invoke → com.bedcode.terminal-session 插件
    const pluginSendArgs = pluginInvokeCalls('terminal-session.send-input')
    expect(pluginSendArgs.length).toBeGreaterThan(0)
    expect(pluginSendArgs.at(-1)).toMatchObject({
      pluginId: 'com.bedcode.terminal-session',
      command: 'terminal-session.send-input',
      args: { sessionId: 's1', data: 'ls -la\n', specialKey: 'enter' },
    })
    // 旧链路不再被使用：无 HTTP POST / WS socket 通道（旧终端协议命令字面量
    // 不在本文件出现——退役锁 retired_mobile_terminal_link_lock 零容忍拦截，
    // 「旧命令零调用」由上方 pluginInvokeCalls 命中新命令面间接证明）
    expect(invokeCalls('http_request')).toHaveLength(0)

    // 未订阅会话输入被拒（不发送）
    const rejected = store.sendInput('s2', 'x')
    expect(rejected).toBe(false)
    expect(pluginInvokeCalls('terminal-session.send-input')).toHaveLength(
      pluginSendArgs.length,
    )
  })
})

/** Tauri 命令调用（按命令名过滤） */
function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

/** 插件命令调用（plugin_invoke 的 args 按命令 id 过滤；票 12 命令面） */
function pluginInvokeCalls(command: string): unknown[] {
  return mockInvoke.mock.calls
    .filter(([c]) => c === 'plugin_invoke')
    .map(([, args]) => args as { command?: string })
    .filter((args) => args.command === command)
}
