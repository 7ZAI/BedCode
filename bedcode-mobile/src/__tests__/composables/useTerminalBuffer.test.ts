/**
 * useTerminalBuffer.subscribeSession 单元测试
 *
 * 覆盖订阅裁决：字节游标传递、reset（清屏 + 游标重置）、incremental（保留游标）、
 * 已订阅/在途跳过、失败不抛出（返回 null 保持未订阅）。
 * 订阅逻辑已收敛到 store（invoke ws_subscribe_session），wsJoinSession 不再被使用。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

// ensureBuffer → startGlobalListener 会调用 Tauri listen（node 环境无 Tauri internals）
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('@/composables/useMobileCommands', () => ({
  wsLeaveSession: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { wsLeaveSession } from '@/composables/useMobileCommands'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import type { Terminal } from '@xterm/xterm'

/** 等待 listen mock 注册完成（ensureBuffer → startGlobalListener 为异步） */
async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
}

describe('useTerminalBuffer.subscribeSession', () => {
  let store: ReturnType<typeof useTerminalBufferStore>
  let terminalBuffer: ReturnType<typeof useTerminalBuffer>
  const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    terminalBuffer = useTerminalBuffer()
    vi.clearAllMocks()
    invokeMock.mockResolvedValue({
      minSeq: 0,
      maxSeq: 10,
      historyCount: 5,
      mode: 'incremental',
      minOffset: 0,
      maxOffset: 20,
    })
  })

  it('首次订阅：无游标（startSeq null），服务端裁决 reset → 清屏 + 游标重置', async () => {
    const onClear = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput: vi.fn(), onClear })

    invokeMock.mockResolvedValueOnce({
      minSeq: 5,
      maxSeq: 10,
      historyCount: 6,
      mode: 'reset',
      minOffset: 5,
      maxOffset: 20,
    })

    const result = await terminalBuffer.subscribeSession('s1')

    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: null })
    expect(result?.mode).toBe('reset')
    expect(onClear).toHaveBeenCalledTimes(1)
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(true)
  })

  it('有游标时以字节游标续传；incremental 不清屏、游标保留', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 12
    const onClear = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput: vi.fn(), onClear })

    const result = await terminalBuffer.subscribeSession('s1')

    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: 12 })
    expect(result?.mode).toBe('incremental')
    expect(onClear).not.toHaveBeenCalled()
    expect(store.getBuffer('s1')!.cursor).toBe(12) // 游标未被重置
    expect(store.getBuffer('s1')!.subscribed).toBe(true)
  })

  it('已订阅会话跳过，不重复订阅', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')

    const result = await terminalBuffer.subscribeSession('s1')

    expect(result).toBeNull()
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it('订阅失败：不抛出、不标记已订阅（返回 null，等待外部重试）', async () => {
    invokeMock.mockRejectedValueOnce(new Error('network down'))
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const result = await terminalBuffer.subscribeSession('s1')

    expect(result).toBeNull()
    expect(store.getBuffer('s1')!.subscribed).toBe(false)

    warnSpy.mockRestore()
  })

  it('unsubscribeSession：注销 handler + 标记未订阅 + 通知后端离开；游标保留供重进续传', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    store.getBuffer('s1')!.cursor = 7
    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })
    await flushAsync()

    await terminalBuffer.unsubscribeSession('s1')

    expect(wsLeaveSession).toHaveBeenCalledWith('s1')
    const buf = store.getBuffer('s1')!
    expect(buf.subscribed).toBe(false)
    expect(store.realtimeHandlers.has('s1')).toBe(false)
    // 游标保留：重进页面时以字节游标增量续传
    expect(buf.cursor).toBe(7)
  })

  it('unsubscribeSession：wsLeaveSession 失败不抛出（记录警告）', async () => {
    store.ensureBuffer('s1')
    await flushAsync()
    ;(wsLeaveSession as unknown as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error('not connected')
    )
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    await expect(terminalBuffer.unsubscribeSession('s1')).resolves.toBeUndefined()

    warnSpy.mockRestore()
  })

  it('handleDisconnect：标记所有 buffer 未订阅（重连后统一恢复）', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    store.ensureBuffer('s2')
    store.markSubscribed('s2')
    await flushAsync()

    terminalBuffer.handleDisconnect()

    expect(store.getBuffer('s1')!.subscribed).toBe(false)
    expect(store.getBuffer('s2')!.subscribed).toBe(false)
  })

  it('handleSessionStopped：标记停止 + 取消订阅状态 + 离开会话（handler 保留供重启渲染）', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
    await flushAsync()

    await terminalBuffer.handleSessionStopped('s1')

    expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
    expect(store.getBuffer('s1')!.subscribed).toBe(false)
    expect(store.getBuffer('s1')!.cursor).toBe(-1)
    // handler 生命周期归视图（挂载注册/卸载注销）：会话停止不注销，
    // 否则重启后输出链路无 handler 渲染，终端页面永久冻结
    expect(store.realtimeHandlers.has('s1')).toBe(true)
    expect(wsLeaveSession).toHaveBeenCalledWith('s1')
  })

  it('markSessionRunning：复位 sessionStopped 与游标（会话同 id 重启后新流帧可写入）', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    const buf = store.getBuffer('s1')!
    buf.sessionStopped = true
    buf.cursor = 42
    buf.pending = [{ data_base64: 'x', start_offset: 0, end_offset: 1 }] as never
    buf.pendingBytes = 1
    await flushAsync()

    terminalBuffer.markSessionRunning('s1')

    expect(buf.sessionStopped).toBe(false)
    // 订阅与游标一并失效：重启后偏移空间重建，必须走服务端 reset 全量重播
    expect(buf.subscribed).toBe(false)
    expect(buf.cursor).toBe(-1)
    expect(buf.pending).toHaveLength(0)
    expect(buf.pendingBytes).toBe(0)
  })

  it('handleSessionRemoved：清理 buffer 与 handler，并通知后端离开', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
    await flushAsync()

    await terminalBuffer.handleSessionRemoved('s1')

    expect(wsLeaveSession).toHaveBeenCalledWith('s1')
    expect(store.buffers.has('s1')).toBe(false)
    expect(store.realtimeHandlers.has('s1')).toBe(false)
  })

  it('registerRealtimeHandler：onClear 清空 xterm 并释放写队列（writeCoalescer dispose）', async () => {
    const terminal = {
      clear: vi.fn(),
      write: vi.fn(),
      dispose: vi.fn(),
    } as unknown as Terminal

    terminalBuffer.registerRealtimeHandler('s1', terminal)
    const handler = store.realtimeHandlers.get('s1')!
    handler.onClear?.()

    expect(terminal.clear).toHaveBeenCalledTimes(1)
  })

  it('prepareSession：预加载订阅成功 → 标记就绪（终端页 consumePrepared 消费）', async () => {
    // forceReplay 丢弃游标 → startSeq null → 服务端 reset 全量重播
    invokeMock.mockResolvedValueOnce({
      minSeq: 0,
      maxSeq: 10,
      historyCount: 5,
      mode: 'reset',
      minOffset: 0,
      maxOffset: 20,
    })

    const ready = await terminalBuffer.prepareSession('s1')
    expect(ready).toBe(true)
    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: null })
    expect(store.consumePrepared()).toBe('s1')
  })

  it('prepareSession：订阅失败 → 返回 false、不标记就绪（终端页自行重试）', async () => {
    invokeMock.mockRejectedValueOnce(new Error('network down'))
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const ready = await terminalBuffer.prepareSession('s1')
    expect(ready).toBe(false)
    expect(store.consumePrepared()).toBeNull()

    warnSpy.mockRestore()
  })

  it('prepareSession：订阅挂起超时 → 返回 false，不阻塞跳转', async () => {
    vi.useFakeTimers()
    try {
      // 服务端不响应（订阅请求永不返回）
      invokeMock.mockImplementation(() => new Promise(() => {}))

      const pending = terminalBuffer.prepareSession('s1')
      await vi.advanceTimersByTimeAsync(8000)
      const ready = await pending
      expect(ready).toBe(false)
      expect(store.consumePrepared()).toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })
})
