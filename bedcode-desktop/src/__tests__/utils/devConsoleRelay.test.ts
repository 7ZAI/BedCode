/**
 * devConsoleRelay 单元测试：序列化、装/卸载、批量转发链路
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { formatConsoleArgs, installDevConsoleRelay } from '@/utils/devConsoleRelay'
import { invoke } from '@/utils/invoke'

vi.mock('@/utils/invoke', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))

const mockInvoke = vi.mocked(invoke)

describe('formatConsoleArgs', () => {
  it('拼接多参数为空格分隔单行', () => {
    expect(formatConsoleArgs(['a', 1, true, null])).toBe('a 1 true null')
  })

  it('Error 取 stack 而非只有 message', () => {
    const err = new Error('boom')
    expect(formatConsoleArgs([err])).toBe(err.stack || err.message)
  })

  it('对象序列化为 JSON', () => {
    expect(formatConsoleArgs([{ code: 2, name: 'x' }])).toBe('{"code":2,"name":"x"}')
  })

  it('循环引用对象回退 visited-set 序列化，不抛错且保留字段', () => {
    const circular: Record<string, unknown> = { name: 'loop' }
    circular.self = circular
    expect(() => formatConsoleArgs([circular])).not.toThrow()
    const rendered = formatConsoleArgs([circular])
    expect(rendered).toContain('"name":"loop"')
    expect(rendered).toContain('[Circular]')
  })

  it('undefined 显式标记', () => {
    expect(formatConsoleArgs([undefined])).toBe('undefined')
  })
})

describe('installDevConsoleRelay', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('非 dev 环境 no-op：console 不被替换', () => {
    const logSpy = vi.spyOn(console, 'log')
    installDevConsoleRelay(false)
    console.log('hello')
    // 未被替换时原样输出，且不会触发 IPC
    expect(logSpy).toHaveBeenCalledWith('hello')
    expect(mockInvoke).not.toHaveBeenCalled()
    logSpy.mockRestore()
  })

  it('dev 环境：转发前先调原始 console，定时 flush 批量发送', () => {
    const logSpy = vi.spyOn(console, 'log')
    const detach = installDevConsoleRelay(true)

    console.log('init ok')
    expect(logSpy).toHaveBeenCalledWith('init ok')
    expect(mockInvoke).not.toHaveBeenCalled() // 未到阈值，等待定时 flush

    vi.advanceTimersByTime(400)
    expect(mockInvoke).toHaveBeenCalledTimes(1)
    expect(mockInvoke).toHaveBeenCalledWith('report_frontend_log', {
      logs: [{ level: 'debug', message: 'init ok' }],
    })

    detach()
    logSpy.mockRestore()
  })

  it('达到条数阈值立即发送，无需等待定时器', () => {
    const detach = installDevConsoleRelay(true)

    const messages = Array.from({ length: 50 }, (_, i) => `msg ${i}`)
    for (const m of messages) console.info(m)

    expect(mockInvoke).toHaveBeenCalledTimes(1)
    const batch = mockInvoke.mock.calls[0][1]!.logs as { level: string; message: string }[]
    expect(batch).toHaveLength(50)
    expect(batch[49]).toEqual({ level: 'info', message: 'msg 49' })

    detach()
  })

  it('detach 恢复原始 console 方法', () => {
    const detach = installDevConsoleRelay(true)
    console.log = (() => {}) as typeof console.log // 确保已替换
    detach()

    const logSpy = vi.spyOn(console, 'log')
    console.log('after detach', 1)
    // 恢复后走原生 console.log（spy 捕获），不再转发
    expect(logSpy).toHaveBeenCalledWith('after detach', 1)
    logSpy.mockRestore()
  })

  it('转发失败仅静默一次警告，不递归 console.error', async () => {
    const errorSpy = vi.spyOn(console, 'error')
    const warnSpy = vi.spyOn(console, 'warn')
    mockInvoke.mockRejectedValueOnce(new Error('ipc down'))

    const detach = installDevConsoleRelay(true)
    console.error('will fail to relay')
    console.error('second one goes in same batch')

    // 第一次 flush：invoke 被拒 → 仅警告一次（原始 console.error 不受影响）
    await vi.advanceTimersByTimeAsync(400)
    expect(errorSpy).toHaveBeenCalledTimes(2)
    expect(warnSpy).toHaveBeenCalledTimes(1)

    // 第二次 flush：无新条目，不再触发警告
    await vi.advanceTimersByTimeAsync(400)
    expect(warnSpy).toHaveBeenCalledTimes(1)

    errorSpy.mockRestore()
    warnSpy.mockRestore()
    detach()
  })
})