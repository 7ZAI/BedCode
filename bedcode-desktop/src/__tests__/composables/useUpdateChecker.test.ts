/**
 * useUpdateChecker 单元测试（票 04 / ADR 0030）
 *
 * 行为契约：
 * - 检查/下载失败 → status='failed'（UI 显示通用文案），**不再暴露原始错误**
 *   （组件返回对象无 errorMessage 字段——裸错误曾渲染进设置页，已退役）
 * - 技术详情唯一落点 = logger.error（release 下仍转发落盘，见 frontendLogger）
 * - 成功/无更新/进行中等状态映射不受影响
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { configureLogger, resetLoggerState, logger } from '@/utils/frontendLogger'

// ==================== mock Tauri updater / process ====================

const mockCheck = vi.fn()
const mockRelaunch = vi.fn()
let mockDownloadAndInstall = vi.fn()

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: (...args: unknown[]) => mockCheck(...args),
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}))

vi.mock('@/locales', async () => {
  const actual = await vi.importActual<typeof import('@/locales')>('@/locales')
  return actual
})

import { useUpdateChecker } from '@/composables/useUpdateChecker'

// ==================== 基建 ====================

let errorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  resetLoggerState()
  configureLogger(true) // 显式 dev（不依赖 import.meta）
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  mockCheck.mockReset()
  mockRelaunch.mockReset()
  mockDownloadAndInstall = vi.fn().mockResolvedValue(undefined)
})

afterEach(() => {
  errorSpy.mockRestore()
  resetLoggerState()
})

describe('checkForUpdate', () => {
  it('成功发现有更新 → status=available', async () => {
    mockCheck.mockResolvedValueOnce({ version: '1.2.3' })
    const { status, checkForUpdate } = useUpdateChecker()

    const update = await checkForUpdate()

    expect(update).toEqual({ version: '1.2.3' })
    expect(status.value).toBe('available')
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it('成功但无更新 → status=latest', async () => {
    mockCheck.mockResolvedValueOnce(null)
    const { status, checkForUpdate } = useUpdateChecker()

    const update = await checkForUpdate()

    expect(update).toBeNull()
    expect(status.value).toBe('latest')
  })

  it('失败 → status=failed 且原始错误只进日志（不暴露给调用方 UI）', async () => {
    const raw = new Error('tls error: certificate expired at 2026-09-27')
    mockCheck.mockRejectedValueOnce(raw)
    const checker = useUpdateChecker()

    const update = await checker.checkForUpdate()

    expect(update).toBeNull()
    expect(checker.status.value).toBe('failed')
    // 组件不再对外暴露可渲染的裸错误字段（曾：errorMessage = e.message）
    expect('errorMessage' in checker).toBe(false)
    // 技术详情唯一落点 = logger.error
    expect(errorSpy).toHaveBeenCalledWith('[update-checker] checkForUpdate failed:', raw)
    // UI 文案来源是 status 映射的通用文案，绝不含原始错误
    expect(checker.getUpdateStatusText()).toBe('检查更新失败，请稍后重试')
    expect(checker.getUpdateStatusText()).not.toContain('tls error')
  })

  it('失败后重新检查成功 → 状态恢复（不残留 failed）', async () => {
    mockCheck.mockRejectedValueOnce(new Error('network down'))
    mockCheck.mockResolvedValueOnce(null)
    const { status, checkForUpdate } = useUpdateChecker()

    await checkForUpdate()
    expect(status.value).toBe('failed')

    await checkForUpdate()
    expect(status.value).toBe('latest')
  })
})

describe('downloadAndInstall', () => {
  it('成功下载+安装 → 状态走 downloaded → installing → relaunch', async () => {
    const update = {
      downloadAndInstall: mockDownloadAndInstall,
    }
    mockCheck.mockResolvedValueOnce(update)
    const { status, downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall()

    expect(mockDownloadAndInstall).toHaveBeenCalled()
    expect(mockRelaunch).toHaveBeenCalled()
    expect(status.value).toBe('installing')
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it('下载/安装失败 → status=failed 且详情只进日志', async () => {
    const update = { downloadAndInstall: mockDownloadAndInstall }
    const raw = new Error('checksum mismatch: expected abc123')
    mockCheck.mockResolvedValueOnce(update)
    mockDownloadAndInstall.mockRejectedValueOnce(raw)
    const { status, downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall()

    expect(status.value).toBe('failed')
    expect(errorSpy).toHaveBeenCalledWith('[update-checker] downloadAndInstall failed:', raw)
    expect(useUpdateChecker().getUpdateStatusText()).not.toContain('checksum')
  })
})

describe('getUpdateStatusText（UI 唯一文案来源）', () => {
  it('failed → 通用文案且不含任何技术字段', () => {
    const { status, getUpdateStatusText } = useUpdateChecker()
    status.value = 'failed'
    expect(getUpdateStatusText()).toBe('检查更新失败，请稍后重试')
  })

  it('其余状态映射稳定', () => {
    const { status, getUpdateStatusText } = useUpdateChecker()
    status.value = 'idle'
    expect(getUpdateStatusText()).toBe('检查更新')
    status.value = 'checking'
    expect(getUpdateStatusText()).toBe('正在检查更新...')
    status.value = 'available'
    expect(getUpdateStatusText()).toBe('发现新版本')
    status.value = 'latest'
    expect(getUpdateStatusText()).toBe('已是最新版本')
    status.value = 'downloading'
    expect(getUpdateStatusText()).toBe('正在下载更新...')
    status.value = 'downloaded'
    expect(getUpdateStatusText()).toBe('下载完成，正在安装...')
    status.value = 'installing'
    expect(getUpdateStatusText()).toBe('正在安装更新...')
  })
})