/**
 * frontendLogger 单元测试：序列化、dev 转发链路、release 空函数、级别映射
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  formatLogArgs,
  configureLogger,
  resetLoggerState,
  logger,
  FLUSH_THRESHOLD,
  FLUSH_INTERVAL_MS,
} from '@/utils/frontendLogger'

vi.mock('@/utils/invoke', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

const mockInvoke = vi.fn()

describe('formatLogArgs', () => {
  it('拼接多参数为空格分隔单行', () => {
    expect(formatLogArgs(['a', 1, true, null])).toBe('a 1 true null')
  })

  it('Error 取 stack 而非只有 message', () => {
    const err = new Error('boom')
    expect(formatLogArgs([err])).toBe(err.stack || err.message)
  })

  it('对象序列化为 JSON', () => {
    expect(formatLogArgs([{ code: 2, name: 'x' }])).toBe('{"code":2,"name":"x"}')
  })

  it('循环引用对象回退 visited-set 序列化，不抛错且保留字段', () => {
    const circular: Record<string, unknown> = { name: 'loop' }
    circular.self = circular
    expect(() => formatLogArgs([circular])).not.toThrow()
    const rendered = formatLogArgs([circular])
    expect(rendered).toContain('"name":"loop"')
    expect(rendered).toContain('[Circular]')
  })

  it('undefined 显式标记', () => {
    expect(formatLogArgs([undefined])).toBe('undefined')
  })
})

describe('frontendLogger dev 转发链路', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    resetLoggerState()
    configureLogger(true) // 显式 dev
    mockInvoke.mockReset().mockResolvedValue(undefined)
  })

  afterEach(() => {
    vi.useRealTimers()
    resetLoggerState()
    vi.restoreAllMocks()
  })

  it('dev：logger.info 转发到 Rust 落盘（批量 + 定时 flush）', () => {
    const logSpy = vi.spyOn(console, 'info').mockImplementation(() => {})
    logger.info('init ok')
    // 未到阈值，等待定时 flush
    expect(mockInvoke).not.toHaveBeenCalled()

    vi.advanceTimersByTime(FLUSH_INTERVAL_MS)
    expect(mockInvoke).toHaveBeenCalledTimes(1)
    expect(mockInvoke).toHaveBeenCalledWith('report_frontend_log', {
      logs: [{ level: 'info', message: 'init ok' }],
    })
    logSpy.mockRestore()
  })

  it('dev：达到条数阈值立即发送，无需等待定时器', () => {
    const messages = Array.from({ length: FLUSH_THRESHOLD }, (_, i) => `msg ${i}`)
    for (const m of messages) logger.info(m)

    expect(mockInvoke).toHaveBeenCalledTimes(1)
    const batch = mockInvoke.mock.calls[0][1].logs as { level: string; message: string }[]
    expect(batch).toHaveLength(FLUSH_THRESHOLD)
    expect(batch[FLUSH_THRESHOLD - 1]).toEqual({ level: 'info', message: `msg ${FLUSH_THRESHOLD - 1}` })
  })

  it('dev：level 映射正确（error→error，warn→warn，log→debug）', () => {
    logger.error('boom')
    logger.warn('careful')
    logger.log('plain')

    vi.advanceTimersByTime(FLUSH_INTERVAL_MS)
    expect(mockInvoke).toHaveBeenCalledTimes(1)
    const batch = mockInvoke.mock.calls[0][1].logs as { level: string; message: string }[]
    expect(batch.map((e) => e.level)).toEqual(['error', 'warn', 'debug'])
  })

  it('release：logger 为空函数，零输出零转发', () => {
    configureLogger(false)
    const logSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    logger.error('should not appear')
    expect(logSpy).not.toHaveBeenCalled()
    expect(mockInvoke).not.toHaveBeenCalled()
    logSpy.mockRestore()
  })

  it('转发失败仅静默一次警告，不递归 console.error', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // spy 创建后重新 configureLogger：让 methodFactory 重新捕获 spy（否则闭包里的 original 是模块加载时的原生函数）
    configureLogger(true)
    mockInvoke.mockRejectedValueOnce(new Error('ipc down'))

    logger.error('will fail to relay')
    logger.error('second one goes in same batch')

    // 第一次 flush：invoke 被拒 → 仅警告一次（原始 console.error 不受影响）
    await vi.advanceTimersByTimeAsync(FLUSH_INTERVAL_MS)
    expect(errorSpy).toHaveBeenCalledTimes(2)
    expect(warnSpy).toHaveBeenCalledTimes(1)

    // 第二次 flush：无新条目，不再触发警告
    mockInvoke.mockResolvedValue(undefined)
    await vi.advanceTimersByTimeAsync(FLUSH_INTERVAL_MS)
    expect(warnSpy).toHaveBeenCalledTimes(1)

    errorSpy.mockRestore()
    warnSpy.mockRestore()
  })
})