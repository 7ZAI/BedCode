/**
 * 宿主连接生命周期事件面 行为契约测试
 * （票 2026-10-09-mobile-host-into-wasm-apps，阶段 A4：宿主页下沉的引擎事实投影扩容）
 *
 * 被测：`src/plugin/connection-events.ts`（`mobileApi.onConnectionEvent` 的实现）。
 * 设计约束：插件不得裸听宿主内部事件名 ⇒ 只有白名单 6 个 `ws_*` 事件被封装，
 * 载荷映射为 camelCase 形状；只投引擎事实（重连节奏 / 认证拒绝 / 事件通道就绪）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-E1 | register 调用表 | 恰好订阅白名单 6 个事件，无越界事件 | listen 收到 6 个名字 |
 * | C-E2 | ws_reconnecting 映射 | retry/max_retry → retry/maxRetry，缺字段兜底 0 | {type:'reconnecting',retry,maxRetry} |
 * | C-E3 | 三个无载荷事件 | 原样映射为独立 type | reconnected / unexpected_disconnect / event_channel_ready |
 * | C-E4 | 认证类事件 | reason 透传，缺失兜底空串 | {type:'reauth_rejected'\|'reconnect_failed',reason} |
 * | C-E5 | dispose | 注销后不再回调；重复 dispose 幂等 | 回调计数不增；unlisten 各一次 |
 * | C-E6 | listen 失败 | 非 Tauri 环境降级：不抛异常 + warn 落日志 | 不 reject；logger.warn 一条 |
 * | C-E7 | 在途注册 | dispose 早于 listen resolve 时，resolve 后立即注销 | unlisten 被调用 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

/** 事件替身：listen 登记 handler，注销后不再投递（与真实 Tauri 契约一致） */
interface Entry {
  name: string
  fn: (event: { payload: unknown }) => void
  active: boolean
}
const entries: Entry[] = []
const unlistenNames: string[] = []
/** 'resolve' 立即完成；'reject' 模拟非 Tauri 环境；'pending' 手动控制 resolve 时机 */
let listenMode: 'resolve' | 'reject' | 'pending'
const pendingResolvers: Array<() => void> = []

const mockListen = vi.fn((name: string, fn: (event: { payload: unknown }) => void) => {
  const entry: Entry = { name, fn, active: true }
  entries.push(entry)
  if (listenMode === 'reject') return Promise.reject(new Error('no __TAURI_INTERNALS__'))
  const unlisten = async () => {
    entry.active = false
    unlistenNames.push(name)
  }
  if (listenMode === 'pending') {
    return new Promise<() => Promise<void>>((resolve) => {
      pendingResolvers.push(() => resolve(unlisten))
    })
  }
  return Promise.resolve(unlisten)
})

vi.mock('@tauri-apps/api/event', () => ({ listen: (...args: any[]) => mockListen(...args) }))

const mockLoggerWarn = vi.fn()
vi.mock('@/utils/frontendLogger', () => ({
  logger: { warn: (...args: any[]) => mockLoggerWarn(...args), error: vi.fn(), info: vi.fn() },
}))

import { subscribeConnectionEvents, type HostConnectionEvent } from '@/plugin/connection-events'

/** 投递一个事件（只投到仍登记的 handler） */
function emit(name: string, payload?: unknown): void {
  for (const entry of [...entries]) {
    if (entry.active && entry.name === name) entry.fn({ payload })
  }
}

beforeEach(() => {
  entries.length = 0
  unlistenNames.length = 0
  pendingResolvers.length = 0
  listenMode = 'resolve'
  mockListen.mockClear()
  mockLoggerWarn.mockClear()
})

describe('C-E1 订阅白名单', () => {
  it('should_subscribeExactlyWhitelistedEvents_when_subscribed', () => {
    subscribeConnectionEvents(() => {})

    const names = mockListen.mock.calls.map((c) => c[0]).sort()
    expect(names).toEqual([
      'ws_event_channel_ready',
      'ws_reauth_rejected',
      'ws_reconnect_failed',
      'ws_reconnected',
      'ws_reconnecting',
      'ws_unexpected_disconnect',
    ])
    expect(mockListen).toHaveBeenCalledTimes(6)
  })
})

describe('C-E2 reconnecting 载荷映射', () => {
  it('should_mapRetryFields_when_payloadGiven', () => {
    const received: HostConnectionEvent[] = []
    subscribeConnectionEvents((e) => received.push(e))

    emit('ws_reconnecting', { retry: 3, max_retry: 5 })
    expect(received).toEqual([{ type: 'reconnecting', retry: 3, maxRetry: 5 }])
  })

  it('should_defaultRetryFieldsToZero_when_payloadMissing', () => {
    const received: HostConnectionEvent[] = []
    subscribeConnectionEvents((e) => received.push(e))

    emit('ws_reconnecting')
    expect(received).toEqual([{ type: 'reconnecting', retry: 0, maxRetry: 0 }])
  })
})

describe('C-E3 无载荷事件映射', () => {
  it('should_mapEachEventToItsOwnType_when_payloadlessEventsArrive', () => {
    const received: HostConnectionEvent[] = []
    subscribeConnectionEvents((e) => received.push(e))

    emit('ws_reconnected')
    emit('ws_unexpected_disconnect')
    emit('ws_event_channel_ready')

    expect(received).toEqual([
      { type: 'reconnected' },
      { type: 'unexpected_disconnect' },
      { type: 'event_channel_ready' },
    ])
  })
})

describe('C-E4 认证类事件 reason 透传', () => {
  it('should_passReasonThrough_when_reasonGiven', () => {
    const received: HostConnectionEvent[] = []
    subscribeConnectionEvents((e) => received.push(e))

    emit('ws_reauth_rejected', { reason: 'token revoked' })
    emit('ws_reconnect_failed', { reason: 'unreachable' })

    expect(received).toEqual([
      { type: 'reauth_rejected', reason: 'token revoked' },
      { type: 'reconnect_failed', reason: 'unreachable' },
    ])
  })

  it('should_defaultReasonToEmptyString_when_reasonMissing', () => {
    const received: HostConnectionEvent[] = []
    subscribeConnectionEvents((e) => received.push(e))

    emit('ws_reauth_rejected')
    expect(received).toEqual([{ type: 'reauth_rejected', reason: '' }])
  })
})

describe('C-E5 dispose 语义', () => {
  it('should_stopDeliveringAndUnlistenOnce_when_disposed', async () => {
    const received: HostConnectionEvent[] = []
    const disposable = subscribeConnectionEvents((e) => received.push(e))
    await Promise.resolve()

    emit('ws_reconnected')
    expect(received).toHaveLength(1)

    disposable.dispose()
    expect(unlistenNames).toHaveLength(6)

    emit('ws_reconnected')
    expect(received).toHaveLength(1)

    // 重复 dispose 幂等：不重复注销（否则会摘掉别人的监听）
    disposable.dispose()
    expect(unlistenNames).toHaveLength(6)
  })
})

describe('C-E6 非 Tauri 环境降级', () => {
  it('should_notThrowAndWarn_when_listenRejects', async () => {
    listenMode = 'reject'
    const received: HostConnectionEvent[] = []

    expect(() => subscribeConnectionEvents((e) => received.push(e))).not.toThrow()
    await vi.waitFor(() => expect(mockLoggerWarn).toHaveBeenCalledTimes(6))

    // 每条事件各自记一条 warn（含事件名，便于排障），且没有任何事实投递给调用方
    const warnText = mockLoggerWarn.mock.calls.map((c) => String(c[0])).join('\n')
    for (const name of [
      'ws_reconnecting',
      'ws_reconnected',
      'ws_unexpected_disconnect',
      'ws_reauth_rejected',
      'ws_reconnect_failed',
      'ws_event_channel_ready',
    ]) {
      expect(warnText).toContain(name)
    }
    expect(received).toEqual([])
  })
})

describe('C-E7 在途注册的注销', () => {
  it('should_unlistenImmediately_when_disposedBeforeListenResolves', async () => {
    listenMode = 'pending'
    const disposable = subscribeConnectionEvents(() => {})
    expect(entries).toHaveLength(6)

    disposable.dispose()
    expect(unlistenNames).toEqual([])

    // listen 此刻才 resolve：必须立即注销，不留悬挂监听
    for (const resolve of pendingResolvers) resolve()
    await vi.waitFor(() => expect(unlistenNames).toHaveLength(6))
  })
})
