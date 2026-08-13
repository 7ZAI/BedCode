/**
 * terminalBuffer store 单元测试
 *
 * 覆盖：字节游标推进、订阅确认前缓冲回放帧（裁决消息与历史帧乱序竞态）、
 * 连续性不变量违反 → 清屏 + 重新订阅自愈、subscribing 防重、
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

/** 默认订阅裁决（incremental） */
const subscribeOk = {
  minSeq: 0,
  maxSeq: 10,
  historyCount: 5,
  mode: 'incremental',
  minOffset: 0,
  maxOffset: 20,
} as const

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
    invokeMock.mockResolvedValue(subscribeOk)
    vi.clearAllMocks()
    listenMock.mockImplementation((_name: string, cb: (e: { payload: OutputPayload }) => void) => {
      listener = cb
      return Promise.resolve(() => {})
    })
    invokeMock.mockResolvedValue(subscribeOk)
  })

  it('已订阅后字节游标随帧推进，handler 收到输出', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
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

  it('订阅确认前回放帧缓冲（不写入）；确认后跳过快照已覆盖帧，不重复写入', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 0
    await flushAsync()

    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })

    // 订阅请求在途，旧流残留帧先于 subscribe_response 到达 → 缓冲不写入
    listener!({ payload: payload('s1', 'ab', 0, 2) })
    listener!({ payload: payload('s1', 'cd', 2, 4) })
    expect(onOutput).not.toHaveBeenCalled()
    expect(store.getBuffer('s1')!.pending.length).toBe(2)

    // 订阅确认（incremental）：快照 maxOffset=4 已覆盖缓冲帧 [0,4) →
    // 跳过（服务端历史回放会重发这些字节，写入即重复 → 连续性违反闪屏）
    const result = await store.subscribeSession('s1')
    expect(result?.mode).toBe('incremental')
    expect(onOutput).not.toHaveBeenCalled()
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(0)
    expect(buf.pending.length).toBe(0)

    // 服务端回放帧（覆盖帧重发 + 新帧）到达：与游标字节级衔接，按序写入
    listener!({ payload: payload('s1', 'ab', 0, 2) })
    listener!({ payload: payload('s1', 'cd', 2, 4) })
    listener!({ payload: payload('s1', 'ef', 4, 6) })
    expect(onOutput).toHaveBeenCalledTimes(3)
    expect(buf.cursor).toBe(6)
  })

  it('reset 订阅：丢弃确认前缓冲的旧流残留帧，等待新回放帧锚定', async () => {
    store.ensureBuffer('s1')
    await flushAsync()

    const onOutput = vi.fn()
    const onClear = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput, onClear })

    // 订阅往返期间到达的帧只能是旧流残留（响应先于新回放帧发送）——
    // reset 裁决下必须丢弃，否则排空会推进游标导致新回放首帧违反连续性
    listener!({ payload: payload('s1', 'stale', 100, 112) })
    invokeMock.mockResolvedValueOnce({
      minSeq: 0,
      maxSeq: 10,
      historyCount: 5,
      mode: 'reset',
      minOffset: 5,
      maxOffset: 30,
    })

    await store.subscribeSession('s1')

    expect(onClear).toHaveBeenCalledTimes(1)
    // 残留帧未写入，等待新回放帧（游标 -1 首帧锚定）
    expect(onOutput).not.toHaveBeenCalled()
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(true)
    expect(buf.pending.length).toBe(0)

    // 新回放帧（快照点起播）到达：游标 -1 跳过校验，锚定后推进
    listener!({ payload: payload('s1', 'replay', 10, 22) })
    expect(onOutput).toHaveBeenCalledTimes(1)
    expect(buf.cursor).toBe(22)
  })

  it('连续性违反（已订阅后）：丢弃游标 + 重新订阅（裁决 reset 确认后清屏）', async () => {
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    store.getBuffer('s1')!.cursor = 5
    await flushAsync()

    const onClear = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput: vi.fn(), onClear })

    // 帧起点 6 而非 5 → 不变量破坏
    listener!({ payload: payload('s1', 'xy', 6, 8) })

    // 同步部分：游标/订阅状态重置；清屏推迟到裁决确认后（自愈失败时
    // 不提前清屏 → 不留永久黑屏，旧内容展示到新回放到达）
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(false)
    expect(onClear).not.toHaveBeenCalled()

    // 自愈重订阅（doSubscribe 先 await 监听器注册，invoke 异步发出）；
    // 游标已丢弃 → startSeq null → 服务端裁决 reset 全量重播
    invokeMock.mockResolvedValueOnce({
      minSeq: 0,
      maxSeq: 10,
      historyCount: 5,
      mode: 'reset',
      minOffset: 5,
      maxOffset: 30,
    })
    await flushAsync()
    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: null })
    expect(buf.subscribed).toBe(true)
    expect(onClear).toHaveBeenCalledTimes(1)
  })

  it('订阅请求在途时重复订阅跳过（subscribing 防重）', async () => {
    store.ensureBuffer('s1')
    await flushAsync()

    let resolveInvoke: ((v: unknown) => void) | undefined
    invokeMock.mockImplementation(
      () => new Promise((r) => { resolveInvoke = r })
    )

    const first = store.subscribeSession('s1')
    const second = store.subscribeSession('s1')
    // 第一次订阅先 await 监听器注册，invoke 在微任务后发出；第二次调用直接跳过
    await flushAsync()
    expect(invokeMock).toHaveBeenCalledTimes(1)

    resolveInvoke!(subscribeOk)
    await first
    await second
    expect(store.getBuffer('s1')!.subscribed).toBe(true)
  })

  it('订阅失败：丢弃缓冲帧，保持未订阅（等待外部重试）', async () => {
    store.ensureBuffer('s1')
    await flushAsync()

    // 订阅确认前先到达回放帧（缓冲）
    listener!({ payload: payload('s1', 'ab', 0, 2) })

    invokeMock.mockRejectedValueOnce(new Error('network down'))
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const result = await store.subscribeSession('s1')

    expect(result).toBeNull()
    const buf = store.getBuffer('s1')!
    expect(buf.subscribed).toBe(false)
    expect(buf.pending.length).toBe(0) // 缓冲帧已丢弃

    warnSpy.mockRestore()
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
    store.markSubscribed('s1')
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

  it('forceReplay：重置游标与订阅状态，下次订阅服务端裁决 reset 全量重播', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 100
    store.markSubscribed('s1')
    await flushAsync()

    store.forceReplay('s1')
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(false)
    expect(buf.pending.length).toBe(0)

    invokeMock.mockResolvedValueOnce({ ...subscribeOk, mode: 'reset' })
    const result = await store.subscribeSession('s1')

    // 游标丢弃 → startSeq null → 服务端全量重播
    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: null })
    expect(result?.mode).toBe('reset')
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(true)
  })

  it('forceReplay 与在途订阅竞态：旧游标 incremental 完成后以重置游标重订阅', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 100
    await flushAsync()

    let resolveInvoke: ((v: unknown) => void) | undefined
    invokeMock.mockImplementation(
      () => new Promise((r) => { resolveInvoke = r })
    )

    // 后台在途订阅（旧游标 100）：doSubscribe 已越过监听器 await、捕获游标、
    // 发出 invoke（此时未到页面重进）
    const bg = store.subscribeSession('s1')
    await flushAsync()
    expect(invokeMock).toHaveBeenCalledTimes(1)

    // 页面重进：强制重播（在途订阅未完成，游标与订阅状态已重置）
    store.forceReplay('s1')
    expect(store.getBuffer('s1')!.cursor).toBe(-1)

    // 在途订阅按旧游标 incremental 完成；重订阅返回 reset（全量重播）
    invokeMock.mockResolvedValueOnce({ ...subscribeOk, mode: 'reset' })
    resolveInvoke!(subscribeOk)
    const result = await bg

    expect(result?.mode).toBe('reset')
    expect(invokeMock).toHaveBeenCalledTimes(2)
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'ws_subscribe_session', { sessionId: 's1', startSeq: 100 })
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'ws_subscribe_session', { sessionId: 's1', startSeq: null })
    const buf = store.getBuffer('s1')!
    expect(buf.subscribed).toBe(true)
    expect(buf.cursor).toBe(-1)
  })

  it('会话停止后游标与订阅状态一并失效（重启后走全量重播）', async () => {
    store.ensureBuffer('s1')
    store.getBuffer('s1')!.cursor = 50
    store.markSubscribed('s1')
    await flushAsync()

    store.markSessionStopped('s1')
    const buf = store.getBuffer('s1')!
    expect(buf.cursor).toBe(-1)
    expect(buf.subscribed).toBe(false)

    // 重启后订阅：游标丢弃 → 全量重播（新流偏移空间从 0 重建，旧游标无效）
    invokeMock.mockResolvedValueOnce({ ...subscribeOk, mode: 'reset' })
    const result = await store.subscribeSession('s1')
    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: null })
    expect(result?.mode).toBe('reset')
  })

  it('预加载（已订阅无 handler）：回放帧缓冲，注册 handler 时回退游标统一写入', async () => {
    store.ensureBuffer('s1')
    await flushAsync()

    // 会话页预加载订阅（forceReplay 后游标丢弃 → reset 全量重播），无 handler
    invokeMock.mockResolvedValueOnce({ ...subscribeOk, mode: 'reset' })
    const result = await store.subscribeSession('s1')
    expect(result?.mode).toBe('reset')
    const buf = store.getBuffer('s1')!
    expect(buf.subscribed).toBe(true)

    // 回放帧到达：无 handler → 缓冲不丢弃，游标正常推进
    const onOutput = vi.fn()
    listener!({ payload: payload('s1', 'ab', 0, 2) })
    listener!({ payload: payload('s1', 'cd', 2, 4) })
    expect(onOutput).not.toHaveBeenCalled()
    expect(buf.cursor).toBe(4)
    expect(buf.pending.length).toBe(2)

    // 终端页挂载注册 handler：回退游标到首帧起点后按序写入
    store.registerRealtimeHandler('s1', { onOutput })
    expect(onOutput).toHaveBeenCalledTimes(2)
    const first = onOutput.mock.calls[0][0] as Uint8Array
    expect(String.fromCharCode(...first)).toBe('ab')
    expect(buf.cursor).toBe(4)
    expect(buf.pending.length).toBe(0)

    // 后续实时帧直达 handler
    listener!({ payload: payload('s1', 'ef', 4, 6) })
    expect(onOutput).toHaveBeenCalledTimes(3)
    expect(buf.cursor).toBe(6)
  })

  it('预加载就绪标记：markPrepared 后 consumePrepared 一次性消费', async () => {
    expect(store.consumePrepared()).toBeNull()
    store.markPrepared('s1')
    expect(store.consumePrepared()).toBe('s1')
    expect(store.consumePrepared()).toBeNull()
  })

  // ==================== 手动暂停 / 恢复（会话卡片） ====================
  // 核心不变量：暂停保留字节游标，恢复按游标续传（服务端裁决 incremental/
  // reset），PTY 输出字节不缺失、不重复（服务端 get_range「严格连续」保证）

  it('暂停：保留游标、清除订阅状态；暂停期间在途帧丢弃（游标不推进、不悄悄重建订阅）', async () => {
    store.ensureBuffer('s1')
    const buf = store.getBuffer('s1')!
    buf.cursor = 10
    await flushAsync()

    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })

    store.pauseSubscription('s1')
    expect(buf.manuallyPaused).toBe(true)
    expect(buf.subscribed).toBe(false)
    // 游标保留：恢复时从该位置续传
    expect(buf.cursor).toBe(10)

    // 取消订阅生效前的在途帧：丢弃，不推进游标、不触发自愈重订阅
    listener!({ payload: payload('s1', 'ab', 10, 12) })
    expect(buf.cursor).toBe(10)
    expect(onOutput).not.toHaveBeenCalled()
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it('暂停期间连续性违反不触发自愈（尺寸控制权不被悄悄抢回）', async () => {
    store.ensureBuffer('s1')
    const buf = store.getBuffer('s1')!
    buf.cursor = 5
    store.markSubscribed('s1')
    store.pauseSubscription('s1')
    await flushAsync()

    // 若入口未拦截，start=6 会触发连续性违反 → 自愈重订阅（invoke 调用）
    listener!({ payload: payload('s1', 'xy', 6, 8) })
    expect(buf.manuallyPaused).toBe(true)
    expect(buf.subscribed).toBe(false)
    expect(buf.cursor).toBe(5)
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it('恢复：以保留游标请求续传（incremental），帧从游标起字节连续写入', async () => {
    store.ensureBuffer('s1')
    const buf = store.getBuffer('s1')!
    buf.cursor = 10
    store.pauseSubscription('s1')
    await flushAsync()

    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput })

    // 会话卡片「恢复订阅」：解除暂停 + 按保留游标订阅
    store.resumeSubscription('s1')
    expect(buf.manuallyPaused).toBe(false)
    await store.subscribeSession('s1')

    // 订阅请求携带保留游标（非 null）→ 服务端 incremental 从 10 字节级续传
    expect(invokeMock).toHaveBeenCalledWith('ws_subscribe_session', { sessionId: 's1', startSeq: 10 })
    expect(buf.subscribed).toBe(true)
    expect(buf.manuallyPaused).toBe(false)

    // 续传帧从游标 10 起（start_offset === cursor 通过连续性校验），不缺失
    listener!({ payload: payload('s1', 'ef', 10, 12) })
    listener!({ payload: payload('s1', 'gh', 12, 14) })
    expect(onOutput).toHaveBeenCalledTimes(2)
    expect(buf.cursor).toBe(14)
  })

  it('恢复：游标被环形淘汰时服务端裁决 reset → 清屏后全量重播（不缺失）', async () => {
    store.ensureBuffer('s1')
    const buf = store.getBuffer('s1')!
    buf.cursor = 10
    store.pauseSubscription('s1')
    await flushAsync()

    const onClear = vi.fn()
    const onOutput = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput, onClear })

    // 暂停期间输出量超过环形保留区间：游标 10 < minOffset → 服务端裁决 reset
    invokeMock.mockResolvedValueOnce({
      minSeq: 0,
      maxSeq: 50,
      historyCount: 40,
      mode: 'reset',
      minOffset: 5,
      maxOffset: 45,
    })
    store.resumeSubscription('s1')
    await store.subscribeSession('s1')

    expect(buf.subscribed).toBe(true)
    expect(buf.cursor).toBe(-1)
    expect(onClear).toHaveBeenCalledTimes(1)

    // 全量回放帧按序写入（起点 0，游标 -1 首帧锚定）
    listener!({ payload: payload('s1', 'full-replay', 0, 11) })
    expect(onOutput).toHaveBeenCalledTimes(1)
    expect(buf.cursor).toBe(11)
  })

  it('断连保留手动暂停；会话停止清除暂停（新生命周期默认正常订阅）', async () => {
    store.ensureBuffer('s1')
    store.pauseSubscription('s1')
    expect(store.getBuffer('s1')!.manuallyPaused).toBe(true)

    // 断连：订阅状态全清，但手动暂停意图保留
    store.markAllUnsubscribed()
    expect(store.getBuffer('s1')!.manuallyPaused).toBe(true)
    expect(store.getBuffer('s1')!.subscribed).toBe(false)

    // 会话停止：暂停理由消失，新会话生命周期正常订阅
    store.markSessionStopped('s1')
    expect(store.getBuffer('s1')!.manuallyPaused).toBe(false)
    expect(store.getBuffer('s1')!.subscribed).toBe(false)
  })

  it('订阅在途时手动暂停：完成后保持暂停不置已订阅（暂停不被静默解除）', async () => {
    store.ensureBuffer('s1')
    const buf = store.getBuffer('s1')!
    buf.cursor = 10
    await flushAsync()

    let resolveInvoke: ((v: unknown) => void) | undefined
    invokeMock.mockImplementation(
      () => new Promise((r) => { resolveInvoke = r })
    )

    // 恢复订阅请求在途（subscribing=true）
    const pending = store.subscribeSession('s1')
    expect(buf.subscribing).toBe(true)
    // 推进微任务：doSubscribe 完成 startGlobalListener 后到达 invoke（挂起中）
    await flushAsync()
    expect(resolveInvoke).toBeDefined()

    // 在途期间用户点了「暂停」：wsLeaveSession 由 composable 发出，
    // store 侧仅标记——在途订阅完成后不得覆盖暂停语义
    store.pauseSubscription('s1')
    expect(buf.manuallyPaused).toBe(true)

    resolveInvoke!(subscribeOk)
    await pending
    await flushAsync()

    // 暂停保持：订阅未被悄悄建立（服务端最终状态由 wsLeaveSession 撤销）
    expect(buf.manuallyPaused).toBe(true)
    expect(buf.subscribed).toBe(false)
  })
})
