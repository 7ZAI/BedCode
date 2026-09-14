/**
 * 终端流组合集成测试（L2 场景 3）
 *
 * 协作实体：真实 xterm Terminal（happy-dom 中 open() 进带尺寸 DOM 元素可用，
 * 已实测 buffer 解析/渲染正常——无需桩）+ useTerminalOutputStreamChannel
 * （真实，帧解析/游标连续性/Channel 订阅构造全部真实执行）+ useSessionStore
 * （真实 Pinia）+ useTerminalInputMarkers（真实，输入标记联动）。
 *
 * 覆盖用户路径：会话创建 → 启动 → 输出流订阅 → 二进制帧渲染到 xterm buffer
 * → 连续性不变量（offset 缺口触发保留游标重订阅）→ 输入回传参数构造（write_to_session
 * 参数）→ 输入标记记录 → 会话终止。
 *
 * 测试 seam：
 * - 只 mock @tauri-apps/api/core 的 invoke 边界 + Channel 类（桌面端终端输出
 *   已整体走 IPC Channel，WS 环回链路（/ws/terminal/local）已下线；Channel
 *   是 IPC 边界，测试内无真实 WebView 推送，帧注入由测试驱动）
 * - xterm / Pinia / composables / store 全部真实执行
 * - fixture 数据取自工厂（makeSessionInfo）
 *
 * 环境限制说明：
 * - 真实 timers：xterm 解析/渲染依赖真实异步 tick，不使用 fake timers
 * - invokeWithTimeout（session store 用）的 30s 兜底 setTimeout 在测试结束
 *   时被 vitest worker 回收，不影响结果
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { Terminal } from '@xterm/xterm'
import {
  useTerminalOutputStreamChannel,
  type OutputStreamFrame,
} from '@/composables/useTerminalOutputStreamChannel'
import { useSessionStore } from '@/stores/session'
import { useTerminalInputMarkers } from '@/composables/useTerminalInputMarkers'
import { makeSessionInfo } from '@/__tests__/fixtures/index'

// ==================== mock Tauri invoke + Channel 边界 ====================

const { mockInvoke, MockChannel } = vi.hoisted(() => {
  const mockInvoke = vi.fn()

  /** Channel mock：记录实例，onmessage 由测试手动触发注入帧（IPC 推送边界桩） */
  class MockChannel {
    static instances: MockChannel[] = []
    onmessage: ((msg: ArrayBuffer) => void) | null = null
    constructor() {
      MockChannel.instances.push(this)
    }
  }

  return { mockInvoke, MockChannel }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  Channel: MockChannel,
}))

/** TB v3 帧缓冲区：start_offset = 帧内首字节累计偏移；len = 字节数（无事件数编码） */
function frameBuffer(bytes: number[], startOffset = 0, isWaiting = false) {
  const buf = new ArrayBuffer(16 + bytes.length)
  const view = new DataView(buf)
  view.setUint8(0, 0x54)
  view.setUint8(1, 0x42)
  view.setUint8(2, 3)
  view.setUint8(3, isWaiting ? 1 : 0)
  view.setBigUint64(4, BigInt(startOffset), true)
  view.setUint32(12, bytes.length, true)
  new Uint8Array(buf, 16).set(bytes)
  return buf
}

/** 快照订阅返回（subscribe_terminal_channel 命令返回值，字段与 ChannelSubscribeResponse 对齐） */
interface SnapshotSpec {
  minOffset: number
  snapshotOffset: number
  historyBytes: number
}

/** 订阅快照队列：每次 subscribe_terminal_channel 调用 shift 一个（模拟不同订阅时刻的历史状态） */
let snapshotQueue: SnapshotSpec[] = []
let clientCounter = 0

/** 等待异步订阅流程 + xterm 解析 tick（真实 timers，xterm 需真实异步推进） */
async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 20))
}

// ==================== 测试基建 ====================

/** 会话 DB（可变） */
let sessionDb: ReturnType<typeof makeSessionInfo>[]

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string, _args?: any) => {
    switch (cmd) {
      case 'subscribe_terminal_channel': {
        const spec = snapshotQueue.shift() ?? { minOffset: 0, snapshotOffset: 5, historyBytes: 3 }
        clientCounter += 1
        return Promise.resolve({
          minOffset: spec.minOffset,
          snapshotOffset: spec.snapshotOffset,
          historyBytes: spec.historyBytes,
          clientId: `channel-session-1-${clientCounter}`,
        })
      }
      case 'unsubscribe_terminal_channel':
        return Promise.resolve(undefined)
      case 'terminal_channel_ack':
        return Promise.resolve(undefined)
      case 'create_session_no_start':
        return Promise.resolve('session-1')
      case 'start_existing_session':
        return Promise.resolve(undefined)
      case 'kill_session':
        return Promise.resolve(undefined)
      case 'list_sessions':
        return Promise.resolve([...sessionDb])
      case 'write_to_session':
        return Promise.resolve(undefined)
      case 'resize_session':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

function invokeCalls(cmd: string): unknown[][] {
  // 去掉调用数组首元素（命令名），只保留参数：与 toHaveBeenCalledWith 的参数形态一致
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

let term: Terminal | null = null
let host: HTMLDivElement | null = null

function createTerminal() {
  host = document.createElement('div')
  host.style.width = '800px'
  host.style.height = '400px'
  document.body.appendChild(host)
  term = new Terminal({ cols: 80, rows: 24 })
  term.open(host)
  return term
}

/** 读取 xterm buffer 指定行文本 */
function bufferLineText(term: Terminal, line: number): string {
  return term.buffer.active.getLine(line)?.translateToString().trim() ?? ''
}

beforeEach(() => {
  vi.clearAllMocks()
  setActivePinia(createPinia())
  sessionDb = []
  snapshotQueue = []
  clientCounter = 0
  MockChannel.instances = []
  installInvokeMock()
})

afterEach(() => {
  term?.dispose()
  term = null
  host?.remove()
  host = null
})

// ==================== 场景 ====================

describe('终端流：xterm × useTerminalOutputStreamChannel × useSessionStore × useTerminalInputMarkers', () => {
  it('会话控制：创建 → 启动（两阶段）→ 终止，invoke 参数与 activeSession 状态联动', async () => {
    const sessionStore = useSessionStore()

    // 两阶段启动第一阶段：创建会话（不启动 PTY）；后端 DB 同步出现 starting 会话
    sessionDb = [makeSessionInfo({ id: 'session-1', status: 'starting' })]
    const sessionId = await sessionStore.createSession('config-1')
    expect(sessionId).toBe('session-1')
    expect(invokeCalls('create_session_no_start')).toEqual([[{ configId: 'config-1' }]])
    expect(sessionStore.sessions).toHaveLength(1)
    expect(sessionStore.sessions[0].status).toBe('starting')

    // 第二阶段：启动已创建会话 → 状态 running + activeSession 联动
    sessionDb = [makeSessionInfo({ id: 'session-1', status: 'running' })]
    await sessionStore.startSession('session-1')
    expect(invokeCalls('start_existing_session')).toEqual([[{ sessionId: 'session-1' }]])
    expect(sessionStore.activeSession?.id).toBe('session-1')
    expect(sessionStore.sessions[0].status).toBe('running')

    // 终止会话 → 列表清空 + activeSession 置空
    sessionDb = []
    await sessionStore.killSession('session-1')
    expect(invokeCalls('kill_session')).toEqual([[{ sessionId: 'session-1' }]])
    expect(sessionStore.sessions).toHaveLength(0)
    expect(sessionStore.activeSession).toBeNull()
  })

  it('输出流渲染：Channel 订阅参数构造 → 二进制帧写入 xterm buffer（含续传帧）', async () => {
    const term = createTerminal()
    const frames: OutputStreamFrame[] = []
    const stream = useTerminalOutputStreamChannel({
      onData: (frame) => {
        frames.push(frame)
        term.write(frame.data)
      },
      onReset: () => term.clear(),
    })

    stream.start('session-1')
    await flushAsync()
    stream.subscribe()
    await flushAsync()
    const ch = MockChannel.instances[0]
    expect(ch).toBeTruthy()

    // 订阅命令参数构造：sessionId + 本次新建的 Channel 实例
    const subCall = invokeCalls('subscribe_terminal_channel')[0]
    expect(subCall[0].sessionId).toBe('session-1')
    expect(subCall[0].channel).toBe(ch)

    // 快照元数据：min 0 ≤ 无游标，直接全量
    expect(invokeCalls('subscribe_terminal_channel')[0][0].channel).toBeTruthy()

    // 事件 startOffset=0 "hi\n" → 渲染到 buffer 第 0 行（\n 换行：xterm 中 \r 仅回车不换行）
    ch.onmessage?.(frameBuffer([104, 105, 10], 0))
    await flushAsync()
    expect(frames).toHaveLength(1)
    expect(bufferLineText(term, 0)).toBe('hi')

    // 后续事件 startOffset=3 "bye" → 字节区间 [3,6) 无缝衔接（渲染到第 1 行）
    ch.onmessage?.(frameBuffer([98, 121, 101], 3))
    await flushAsync()
    expect(frames).toHaveLength(2)
    expect(bufferLineText(term, 1)).toBe('bye')

    stream.stop()
  })

  it('offset 缺口：立即快照重订阅补回缺失字节，冷却期内缺口跳过不渲染', async () => {
    const term = createTerminal()
    const resets: unknown[] = []
    const stream = useTerminalOutputStreamChannel({
      onData: ({ data }) => term.write(data),
      onReset: () => {
        resets.push(true)
        term.clear()
      },
    })

    stream.start('session-1')
    await flushAsync()
    stream.subscribe()
    await flushAsync()
    const ch = MockChannel.instances[0]

    // startOffset=0 "hi\n"、startOffset=3 "bye\n" → 各自换行渲染，游标 = 7
    ch.onmessage?.(frameBuffer([104, 105, 10], 0))
    ch.onmessage?.(frameBuffer([98, 121, 101, 10], 3))
    await flushAsync()
    expect(bufferLineText(term, 0)).toBe('hi')
    expect(bufferLineText(term, 1)).toBe('bye')

    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    // 缺口：startOffset=99 ≠ 游标(7)（字节 7..98 丢失）→ 立即快照重订阅补回
    // （缺口帧不渲染不推进游标——缺失字节无法从实时流恢复，跳过会把残缺序列
    // 写进 buffer 变残渣）；冷却期内再次缺口仅跳过不重复重订阅
    ch.onmessage?.(frameBuffer([120], 99))
    await flushAsync()
    expect(errorSpy).not.toHaveBeenCalled()
    expect(warnSpy).toHaveBeenCalledWith(expect.stringMatching(/offset gap, re-subscribing/))
    expect(bufferLineText(term, 0)).toBe('hi') // 缺口帧未渲染（无残渣写入）

    // 冷却期内（3s）再次缺口：不重复重订阅，缺口帧跳过
    ch.onmessage?.(frameBuffer([121], 200))
    await flushAsync()
    expect(warnSpy).toHaveBeenCalledTimes(1)
    expect(MockChannel.instances.length).toBe(2) // 冷却期内未再触发重订阅（第一次缺口已建 ch2）

    // 重订阅：先精确取消旧订阅（client_id），再建新 Channel 订阅
    const ch2 = MockChannel.instances[1]
    expect(ch2).toBeTruthy()
    expect(invokeCalls('unsubscribe_terminal_channel')).toEqual([
      [{ sessionId: 'session-1', clientId: 'channel-session-1-1' }],
    ])
    expect(invokeCalls('subscribe_terminal_channel')).toHaveLength(2)

    // 快照确认（min_offset=0 ≤ 游标 7：未截断，不清屏）+ 重播帧从游标续补缺失段
    await flushAsync()
    ch2.onmessage?.(frameBuffer([111, 107], 7)) // 重播：恰从游标 7 起补回缺失字节 [7,9)
    await flushAsync()
    expect(bufferLineText(term, 2)).toBe('ok')
    expect(resets).toHaveLength(0) // 未截断，不清屏

    errorSpy.mockRestore()
    warnSpy.mockRestore()
    stream.stop()
  })

  it('历史截断：min_offset > last_rendered_offset → 清屏全量重播 + onTruncated 提示', async () => {
    const term = createTerminal()
    // happy-dom 下 xterm.clear() 会触发渲染器内存爆炸（worker OOM，实测稳定复现）；
    // 清屏属 xterm 自身行为而非被测逻辑，用 spy 拦截真实清屏，仅验证回调联动
    vi.spyOn(term, 'clear').mockImplementation(() => {})
    const resets: unknown[] = []
    const truncated: number[] = []
    const stream = useTerminalOutputStreamChannel({
      onData: ({ data }) => term.write(data),
      onReset: () => {
        resets.push(true)
        term.clear()
      },
      onTruncated: (minOffset) => truncated.push(minOffset),
    })

    stream.start('session-1')
    await flushAsync()
    stream.subscribe()
    await flushAsync()
    const ch = MockChannel.instances[0]

    // 已渲染 startOffset 0、3（游标 = 7）
    ch.onmessage?.(frameBuffer([104, 105, 10], 0))
    ch.onmessage?.(frameBuffer([98, 121, 101, 10], 3))
    await flushAsync()

    // 触发重订阅：注入缺口帧（快照队列下一次订阅返回 min_offset=200 > 游标 7）
    // → 已渲染区域被环形淘汰 → 清屏全量重播
    snapshotQueue.push({ minOffset: 200, snapshotOffset: 300, historyBytes: 3 })
    ch.onmessage?.(frameBuffer([120], 99))
    await flushAsync()

    expect(resets).toHaveLength(1) // 截断 → 清屏
    expect(truncated).toEqual([200])

    // 清屏后全量重播：min_offset=200 起（重订阅的新 Channel 收帧）
    const ch2 = MockChannel.instances[1]
    await flushAsync()
    ch2.onmessage?.(frameBuffer([104, 105, 13], 200)) // "hi\r"
    await flushAsync()
    expect(bufferLineText(term, 0)).toBe('hi')

    stream.stop()
  })

  it('输入回传：write_to_session 参数构造 + 输入标记（xterm IMarker）联动', async () => {
    const term = createTerminal()
    const sessionStore = useSessionStore()
    const inputMarkers = useTerminalInputMarkers()
    sessionDb = [makeSessionInfo({ id: 'session-1', status: 'running' })]
    await sessionStore.loadSessions()

    // 模拟 TerminalPreview 的 onData 接线：单字符输入 + 回车提交记录标记
    let currentLine = ''
    const onData = (data: string) => {
      sessionStore.writeToSession('session-1', data)
      if (data === '\r' || data === '\n') {
        inputMarkers.record(term, currentLine)
        currentLine = ''
      } else if (data.length === 1 && data.charCodeAt(0) >= 32) {
        currentLine += data
      }
    }

    onData('e')
    onData('c')
    onData('h')
    onData('o')
    onData(' ')
    onData('h')
    onData('i')
    onData('\r')

    // 输入回传参数构造：字符直通（未配对设备 —— write 直发，无加密信封）
    expect(invokeCalls('write_to_session')).toEqual([
      [{ sessionId: 'session-1', data: 'e' }],
      [{ sessionId: 'session-1', data: 'c' }],
      [{ sessionId: 'session-1', data: 'h' }],
      [{ sessionId: 'session-1', data: 'o' }],
      [{ sessionId: 'session-1', data: ' ' }],
      [{ sessionId: 'session-1', data: 'h' }],
      [{ sessionId: 'session-1', data: 'i' }],
      [{ sessionId: 'session-1', data: '\r' }],
    ])
    // 输入标记联动：回车记录整行输入
    expect(inputMarkers.visibleMarkers.value).toHaveLength(1)
    expect(inputMarkers.visibleMarkers.value[0].text).toBe('echo hi')
    expect(inputMarkers.visibleMarkers.value[0].line).toBe(0)

    // PTY 尺寸同步参数构造（TerminalPreview onResize 的 store 路径；默认 force=false）
    await sessionStore.resizeSession('session-1', 80, 24)
    expect(invokeCalls('resize_session')).toEqual([
      [{ sessionId: 'session-1', cols: 80, rows: 24, force: false }],
    ])
  })
})
