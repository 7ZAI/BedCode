/**
 * useLogSettings composable 测试（desktop-logging-overhaul 04）
 *
 * mock invoke 断言三个命令的调用参数与结果透传：
 * set_log_level（级别热调）、open_log_dir（打开目录）、save_log_settings（配置持久化）。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'

const invokeMock = vi.fn()

vi.mock('@/utils/invoke', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

import { useLogSettings, type LogSettingsPayload } from '@/composables/useLogSettings'

describe('useLogSettings', () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it('setLogLevel 调用 set_log_level 并透传级别', async () => {
    invokeMock.mockResolvedValueOnce(undefined)
    const { setLogLevel } = useLogSettings()
    await setLogLevel('debug')
    expect(invokeMock).toHaveBeenCalledTimes(1)
    expect(invokeMock).toHaveBeenCalledWith('set_log_level', { level: 'debug' })
  })

  it('openLogDir 调用 open_log_dir（无参数）', async () => {
    invokeMock.mockResolvedValueOnce(undefined)
    const { openLogDir } = useLogSettings()
    await openLogDir()
    expect(invokeMock).toHaveBeenCalledWith('open_log_dir')
  })

  it('saveLogSettings 调用 save_log_settings 并透传 log 段', async () => {
    invokeMock.mockResolvedValueOnce(undefined)
    const { saveLogSettings } = useLogSettings()
    const payload: LogSettingsPayload = {
      fileLevel: 'warn',
      rotation: 'daily',
      maxFiles: 3,
      format: 'json',
      capacityBytes: 268435456,
      consoleInRelease: false,
    }
    await saveLogSettings(payload)
    expect(invokeMock).toHaveBeenCalledWith('save_log_settings', { log: payload })
  })

  it('命令失败时错误向上传播（不吞异常）', async () => {
    invokeMock.mockRejectedValueOnce(new Error('reload failed'))
    const { setLogLevel } = useLogSettings()
    await expect(setLogLevel('info')).rejects.toThrow('reload failed')
  })
})
