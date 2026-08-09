/**
 * terminalBuffer store 单元测试
 *
 * 覆盖：字节游标推进、连续性不变量违反 → 清屏 + 重新订阅自愈、
 * 旧版服务端（无偏移字段）透传兼容。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

// mock Tauri 事件与 invoke
const listenMock = vi.fn()
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}))
const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

import { useTerminalBufferStore, type OutputPayload } from '@/stores/terminalBuffer'

/** 构造 ws_output 载荷 */
function payload(
  sessionId: string,
  data: string,
  startOffset: number | undefined,
  endOffset: number | undefined,
  index = 0,
): OutputPayload {
  return {
    session_id: sessionId,
    data_base64: btoa(data),
    index,
    is_waiting: false,
    start_offset: startOffset,
    end_offset: endOffset,
  }
}

async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
}

describe('terminalBuffer store', () => {
  let store: ReturnType<typeof useTerminalBufferStore>
  let listener: ((event: { payload: OutputPayload }) => void) | null = null

  beforeEach(async () => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    listener = null
    listenMock.mockImplementation((_name: string, cb: (e: { payload: OutputPayload }) => void) => {
      listener = cb
      return Promise.resolve(() => {})
    })
    invokeMock.mockResolvedValue({})
    vi.clearAllMocks()
    listenMock.mockImplementation((_name: string, cb: (e: { payload: OutputPayload }) => void) => {
      listener = cb
      return Promise.resolve(() => {})
    })
  })

  it('字节游标随帧推进，handler 收到输出', async () => {
    store.ensureBuffer('s1')
    await flushAsync()

    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })

    listener!({ payload: payload('s1', 'ab', 0, 2) })
    listener!({ payload: payload('s1', 'cd', 2, 4) })

    expect(store.getBuffer('s1')!.cursor).toBe(4)
    expect(onOutput).toHaveBeenCalledTimes(2)
    const first = onOutput.mock.calls[0][0] as Uint8Array
    expect(String.fromCharCode(...first)).toBe('ab')
  })

  it('连续性违反：清屏 + 丢弃游标 + 重新订阅（服务端裁决）', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 5
    await flushAsync()

    const onClear = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput: vi.fn(), onClear })

    // 帧起点 6 而非 5 → 不变量破坏
    listener!({ payload: payload('s1', 'xy', 6, 8) })

    expect(onClear).toHaveBeenCalledTimes(1)
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(false)
    // 触发重新订阅（游标丢弃 → startSeq null → 全量重播）
    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: null })

    await flushAsync()
    expect(buf.subscribed).toBe(true)
  })

  it('旧版服务端（无偏移字段）：透传输出，不推进游标', async () => {
    store.ensureBuffer('s1')
    await flushAsync()

    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })

    listener!({ payload: payload('s1', 'legacy', undefined, undefined) })

    expect(onOutput).toHaveBeenCalledTimes(1)
    expect(store.getBuffer('s1')!.cursor).toBe(-1)
  })

  it('未访问过的会话（无 buffer）忽略输出', async () => {
    // 显式启动全局监听器（不创建任何 buffer）
    store.startGlobalListener()
    await flushAsync()
    listener!({ payload: payload('other', 'x', 0, 1) })
    expect(store.buffers.has('other')).toBe(false)
  })

  it('会话已停止后忽略输出（游标不推进）', async () => {
    store.ensureBuffer('s1')
    store.markSessionStopped('s1')
    await flushAsync()

    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })

    listener!({ payload: payload('s1', 'zz', 0, 2) })

    expect(onOutput).not.toHaveBeenCalled()
    expect(store.getBuffer('s1')!.cursor).toBe(-1)
  })

  it('连续性自愈重订阅失败：保持未订阅，等待后续生命周期恢复', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 5
    await flushAsync()

    // 重订阅 invoke 失败
    invokeMock.mockRejectedValueOnce(new Error('network down'))
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    listener!({ payload: payload('s1', 'xy', 6, 8) })
    await flushAsync()

    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(false) // 未标记订阅，可被外部重试

    warnSpy.mockRestore()
  })
})
