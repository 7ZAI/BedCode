/**
 * invokeWithTimeout 测试（票 01，ADR 0030）：
 * 超时失败承载 UserError('host.invoke.timeout', { seconds })，不再泄漏命令名；
 * 正常 resolve / 其他 reject 原样透传。
 */
import { describe, it, expect, afterEach, vi } from 'vitest'
import { invokeWithTimeout } from '@/utils/invoke'
import { IPC_TIMEOUT_CODE, UserError } from '@/utils/userError'

const mockTauriInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockTauriInvoke(...args),
}))

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe('invokeWithTimeout', () => {
  it('正例：正常 resolve 原样透传', async () => {
    mockTauriInvoke.mockResolvedValueOnce({ ok: 1 })
    await expect(invokeWithTimeout('some_cmd')).resolves.toEqual({ ok: 1 })
    expect(mockTauriInvoke).toHaveBeenCalledWith('some_cmd', undefined)
  })

  it('正例：非超时 reject 原样透传（不包装不吞掉）', async () => {
    const rejection = { code: 'host.internal', request_id: 'abcd1234' }
    mockTauriInvoke.mockRejectedValueOnce(rejection)
    await expect(invokeWithTimeout('some_cmd')).rejects.toBe(rejection)
  })

  it('边界：超时 → reject UserError(host.invoke.timeout)，params 带秒数且不含命令名', async () => {
    vi.useFakeTimers()
    mockTauriInvoke.mockReturnValueOnce(new Promise(() => {})) // 永不 resolve

    const pending = invokeWithTimeout('server_start', { port: 8080 })
    const assertion = expect(pending).rejects.toSatisfy((e: unknown) => {
      expect(e).toBeInstanceOf(UserError)
      const ue = e as UserError
      expect(ue.code).toBe(IPC_TIMEOUT_CODE)
      expect(ue.params).toEqual({ seconds: 30 })
      expect(ue.message).not.toContain('server_start')
      expect(ue.message).not.toContain('8080')
      return true
    })
    await vi.advanceTimersByTimeAsync(30_000)
    await assertion
  })

  it('边界：自定义 timeoutMs → params 秒数随之换算', async () => {
    vi.useFakeTimers()
    mockTauriInvoke.mockReturnValueOnce(new Promise(() => {}))

    const pending = invokeWithTimeout('slow_cmd', undefined, 5_000)
    const assertion = expect(pending).rejects.toMatchObject({ code: IPC_TIMEOUT_CODE, params: { seconds: 5 } })
    await vi.advanceTimersByTimeAsync(5_000)
    await assertion
  })

  it('反例：未到超时时刻不 reject（命令名不提前泄漏）', async () => {
    vi.useFakeTimers()
    mockTauriInvoke.mockReturnValueOnce(new Promise(() => {}))

    let settled = false
    invokeWithTimeout('slow_cmd').catch(() => {
      settled = true
    })
    await vi.advanceTimersByTimeAsync(10_000)
    // 30s 超时前绝不 settle（若提前 reject 说明实现有误）
    expect(settled).toBe(false)
  })
})