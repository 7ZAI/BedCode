/**
 * useTerminalBuffer.subscribeSession 单元测试
 *
 * 覆盖订阅裁决：字节游标传递、reset（清屏 + 游标重置）、incremental（保留游标）、
 * 已订阅跳过。wsJoinSession 以 mock 替身模拟。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@/composables/useMobileCommands', () => ({
  wsJoinSession: vi.fn(),
  wsLeaveSession: vi.fn(),
}))

// ensureBuffer → startGlobalListener 会调用 Tauri listen（node 环境无 Tauri internals）
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { wsJoinSession } from '@/composables/useMobileCommands'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'

describe('useTerminalBuffer.subscribeSession', () => {
  let store: ReturnType<typeof useTerminalBufferStore>
  let terminalBuffer: ReturnType<typeof useTerminalBuffer>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    terminalBuffer = useTerminalBuffer()
    vi.clearAllMocks()
    ;(wsJoinSession as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      minSeq: 0,
      maxSeq: 10,
      historyCount: 5,
      mode: 'incremental',
      minOffset: 0,
      maxOffset: 20,
    })
  })

  it('首次订阅：无游标（undefined），服务端裁决 reset → 清屏 + 游标重置', async () => {
    const onClear = vi.fn()
    store.registerRealtimeHandler('s1', { onOutput: vi.fn(), onClear })

    ;(wsJoinSession as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      minSeq: 5,
      maxSeq: 10,
      historyCount: 6,
      mode: 'reset',
      minOffset: 5,
      maxOffset: 20,
    })

    const result = await terminalBuffer.subscribeSession('s1')

    expect(wsJoinSession).toHaveBeenCalledWith('s1', undefined)
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

    expect(wsJoinSession).toHaveBeenCalledWith('s1', 12)
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
    expect(wsJoinSession).not.toHaveBeenCalled()
  })

  it('订阅失败：不标记已订阅，允许后续重试', async () => {
    ;(wsJoinSession as unknown as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error('network down')
    )
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    await expect(terminalBuffer.subscribeSession('s1')).rejects.toThrow('network down')
    expect(store.getBuffer('s1')!.subscribed).toBe(false)

    warnSpy.mockRestore()
  })
})
