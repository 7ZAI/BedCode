/**
 * useTerminalSubscription 单元测试（票 05：live 门控 = 收 subscribed + 超时兜底；
 * 订阅重试）
 *
 * 覆盖：
 * 1. 门控：phase 到达 'live'（收 subscribed）+ replayDone + firstFit → settled；
 *    phase 停在中间态（connecting/auth）不提前放行；订阅失败/无输出会话 →
 *    HISTORY_SETTLE_TIMEOUT_MS 超时兜底放行（timeout）。
 * 2. 订阅重试：订阅失败首次 toast + 3s 重试；成功收敛后停止；已订阅期间
 *    重试定时器不重播（fresh subscribe 翻转保护）。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { useTerminalSubscription, HISTORY_SETTLE_TIMEOUT_MS, SUBSCRIBE_RETRY_INTERVAL_MS } from '@/composables/terminal/useTerminalSubscription'
import type { TerminalKernelContext } from '@/composables/terminal/terminalKernel'

const toastMock = vi.hoisted(() => ({ error: vi.fn(), warning: vi.fn(), success: vi.fn(), info: vi.fn() }))
vi.mock('@/composables/useToast', () => ({ useToast: () => toastMock }))
vi.mock('@/locales', () => ({ default: { global: { t: (key: string) => key } } }))

// 事件 listen 捕获（门控需要 terminal-state 事件）
const eventHandlers: Record<string, ((payload: unknown) => void) | null> = {}
const listenMock = vi.fn()
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => (listenMock as (...a: unknown[]) => unknown)(...args),
  emit: vi.fn().mockResolvedValue(undefined),
}))

const cmd = vi.hoisted(() => ({
  terminalSubscribe: vi.fn(async () => {}),
  terminalUnsubscribe: vi.fn(async () => {}),
  terminalUnsubscribeAll: vi.fn(async () => {}),
  terminalRemove: vi.fn(async () => {}),
  terminalSendInput: vi.fn(async () => {}),
  terminalAckRendered: vi.fn(async () => {}),
  terminalPageSubscribe: vi.fn(async () => {}),
  terminalPageUnsubscribe: vi.fn(async () => {}),
  terminalGetState: vi.fn(async (sessionId: string) => ({
    sessionId,
    phase: 'idle',
    cursor: 0,
    acked: 0,
    stopped: false,
    subscribed: false,
  })),
}))
vi.mock('@/composables/useMobileCommands', () => cmd)

function makeCtx(overrides?: Partial<TerminalKernelContext>): TerminalKernelContext {
  return {
    getSessionId: () => 's1',
    isConnected: () => true,
    isSessionActive: () => true,
    terminalRef: { value: null } as any,
    fitAddonRef: { value: null } as any,
    ...overrides,
  } as TerminalKernelContext
}

async function flushAsync(n = 3) {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
}

describe('useTerminalSubscription（票 05：subscribed 门控 + 超时兜底 + 订阅重试）', () => {
  let store: ReturnType<typeof useTerminalBufferStore>

  beforeEach(async () => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    vi.clearAllMocks()
    eventHandlers['terminal-state'] = null
    ;(vi.mocked(listenMock).mockImplementation as any)(async (name: string, cb: (p: unknown) => void) => {
      eventHandlers[name] = cb
      return () => {}
    })
    // 预注册事件监听（subscribeSession 路径）
    await store.subscribeSession('s1')
    cmd.terminalSubscribe.mockClear()
    await flushAsync()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  describe('渲染就绪门控（subscribed = phase=live + replayDone + fit）', () => {
    it('三条件齐备 → settled', async () => {
      const sub = useTerminalSubscription(makeCtx(), { bufferStore: store, subscribeSession: store.subscribeSession })
      sub.armGate()

      // replayDone（新协议立即触发）+ fit
      sub.markReplayDone()
      sub.markFirstFitDone()
      await flushAsync()

      // 未到 live：不提前放行
      let settled = false
      const p = sub.waitForHistoryGate().then((r) => {
        settled = r === 'settled'
      })

      // 收 subscribed → phase=live
      eventHandlers['terminal-state']!({ payload: { session_id: 's1', phase: 'live', detail: 'subscribed' } })
      const result = await Promise.race([p, Promise.resolve('__pending')])
      expect(result === 'settled' || (await p) === undefined).toBe(true)
      await p
      expect(settled).toBe(true)
    })

    it('phase 停在中间态（connecting）：不提前放行（等 subscribed；超时兜底）', async () => {
      vi.useFakeTimers()
      const sub = useTerminalSubscription(makeCtx(), { bufferStore: store, subscribeSession: store.subscribeSession })
      sub.armGate()
      sub.markReplayDone()
      sub.markFirstFitDone()

      eventHandlers['terminal-state']!({ payload: { session_id: 's1', phase: 'connecting' } })
      let gate: string | null = null
      const p = sub.waitForHistoryGate().then((r) => (gate = r))

      // 中间态不提前放行：兜底窗口过半仍未定
      await vi.advanceTimersByTimeAsync(HISTORY_SETTLE_TIMEOUT_MS / 2)
      expect(gate).toBeNull()

      // 8s 兜底窗口到期放行（server 门控自兜底 settle；mask 不悬挂）
      await vi.advanceTimersByTimeAsync(HISTORY_SETTLE_TIMEOUT_MS + 100)
      await p
      expect(gate).toBe('settled')
    })

    it('无输出会话（mock/非活跃）：settleServerHistoryNow 立即放行订阅条件', async () => {
      const sub = useTerminalSubscription(makeCtx(), { bufferStore: store, subscribeSession: store.subscribeSession })
      sub.armGate()
      sub.markReplayDone()
      sub.markFirstFitDone()
      sub.settleServerHistoryNow()
      const result = await sub.waitForHistoryGate()
      expect(result).toBe('settled')
    })
  })

  describe('订阅重试（subscribeWithRetry）', () => {
    it('订阅失败：首次 toast + 定时重试；成功收敛后停止', async () => {
      vi.useFakeTimers()
      // 首次 invoke 失败（Rust 侧错误）
      cmd.terminalSubscribe.mockRejectedValueOnce(new Error('link error'))

      const sub = useTerminalSubscription(makeCtx(), { bufferStore: store, subscribeSession: store.subscribeSession })
      const p = sub.subscribeWithRetry()
      await vi.advanceTimersByTimeAsync(0) // 冲刷失败路径（微任务链）

      expect(toastMock.error).toHaveBeenCalledWith('mobile.terminal.subscribeFailed')

      // 重试窗口到期：Rust 恢复 → 订阅成功 → subscribed 事件收敛
      cmd.terminalSubscribe.mockResolvedValue(undefined)
      await vi.advanceTimersByTimeAsync(SUBSCRIBE_RETRY_INTERVAL_MS + 100)
      eventHandlers['terminal-state']!({ payload: { session_id: 's1', phase: 'live', detail: 'subscribed' } })
      await vi.advanceTimersByTimeAsync(0)

      // 收敛后不再发起新 subscribe（fresh subscribe 翻转保护：重试不再重播）
      const callsAfterSettle = cmd.terminalSubscribe.mock.calls.length
      await vi.advanceTimersByTimeAsync(SUBSCRIBE_RETRY_INTERVAL_MS * 2)
      expect(cmd.terminalSubscribe.mock.calls.length).toBe(callsAfterSettle)
      expect(toastMock.error).toHaveBeenCalledTimes(1)
      sub.disposeSubscription()
      await p
    })

    it('已订阅期间触发 subscribeWithRetry：仍发起一次 fresh subscribe（页面挂载语义）', async () => {
      // 预加载后进入终端页：belief 已真，仍需重播一次
      eventHandlers['terminal-state']!({ payload: { session_id: 's1', phase: 'live', detail: 'subscribed' } })
      const sub = useTerminalSubscription(makeCtx(), { bufferStore: store, subscribeSession: store.subscribeSession })
      await sub.subscribeWithRetry()
      expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s1')
      sub.disposeSubscription()
    })

    it('已停止会话：订阅不发起（早退 null）', async () => {
      store.markSessionStopped('s1')
      const sub = useTerminalSubscription(makeCtx(), { bufferStore: store, subscribeSession: store.subscribeSession })
      await sub.subscribeWithRetry()
      expect(cmd.terminalSubscribe).not.toHaveBeenCalled()
      sub.disposeSubscription()
    })
  })
})