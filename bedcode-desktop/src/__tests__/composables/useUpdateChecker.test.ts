/**
 * useUpdateChecker 单元测试（票 04 / ADR 0030）
 *
 * 行为契约：
 * - 检查/下载失败 → status='failed'（UI 显示通用文案），**不再暴露原始错误**
 *   （组件返回对象无 errorMessage 字段——裸错误曾渲染进设置页，已退役）
 * - 技术详情唯一落点 = logger.error（release 下仍转发落盘，见 frontendLogger）
 * - 成功/无更新/进行中等状态映射不受影响
 * - 传输面契约（前端零资源访问红线）：只 invoke 宿主命令 `check_for_update` /
 *   `install_update`，**不再**触碰 `@tauri-apps/plugin-updater`（能力已随
 *   `updater:default` 权限撤除，Rust 运行期即拒）
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { configureLogger, resetLoggerState, logger } from '@/utils/frontendLogger'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// 进度事件：mock listen，捕获注册的 handler 以便测试手动触发事件
type ProgressHandler = (event: { payload: unknown }) => void
let progressHandlers: ProgressHandler[] = []
const mockUnlisten = vi.fn()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, handler: ProgressHandler) => {
    progressHandlers.push(handler)
    return mockUnlisten
  }),
}))

const mockRelaunch = vi.fn()
vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}))

vi.mock('@/locales', async () => {
  const actual = await vi.importActual<typeof import('@/locales')>('@/locales')
  return actual
})

import { useUpdateChecker } from '@/composables/useUpdateChecker'

/** 手动广播一条下载进度事件（模拟 Rust 侧 `app://update-progress`） */
function emitProgress(payload: {
  downloaded: number
  content_length: number
  finished: boolean
}): void {
  for (const handler of progressHandlers) handler({ payload })
}

// ==================== 基建 ====================

let errorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  resetLoggerState()
  configureLogger(true) // 显式 dev（不依赖 import.meta）
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  mockInvoke.mockReset()
  mockRelaunch.mockReset()
  mockUnlisten.mockReset()
  progressHandlers = []
  // 单例 ref 跨用例共享：复位状态，避免上一例的 status 渗进下一例
  const { status, downloadProgress } = useUpdateChecker()
  status.value = 'idle'
  downloadProgress.value = { downloaded: 0, contentLength: 0 }
})

afterEach(() => {
  errorSpy.mockRestore()
  resetLoggerState()
})

describe('checkForUpdate', () => {
  it('成功发现有更新 → status=available', async () => {
    mockInvoke.mockResolvedValueOnce({ available: true, version: '1.2.3', date: '2026-01-01' })
    const { status, checkForUpdate } = useUpdateChecker()

    const update = await checkForUpdate()

    expect(update).toEqual({ available: true, version: '1.2.3', date: '2026-01-01' })
    expect(status.value).toBe('available')
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it('经宿主命令发起（不碰 updater 插件）', async () => {
    mockInvoke.mockResolvedValueOnce({ available: true, version: '2.0.0', date: null })
    await useUpdateChecker().checkForUpdate()

    expect(mockInvoke).toHaveBeenCalledWith('check_for_update')
  })

  it('成功但无更新 → status=latest 且返回 null（调用方靠 null + status 弹「已是最新版本」）', async () => {
    mockInvoke.mockResolvedValueOnce({ available: false, version: null, date: null })
    const { status, checkForUpdate } = useUpdateChecker()

    const update = await checkForUpdate()

    // 返回真值对象会让 `!update && status==='latest'` 永不成立，toast 静默失效
    expect(update).toBeNull()
    expect(status.value).toBe('latest')
  })

  it('失败 → status=failed 且原始错误只进日志（不暴露给调用方 UI）', async () => {
    const raw = new Error('tls error: certificate expired at 2026-09-27')
    mockInvoke.mockRejectedValueOnce(raw)
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
    mockInvoke.mockRejectedValueOnce(new Error('network down'))
    mockInvoke.mockResolvedValueOnce({ available: false, version: null, date: null })
    const { status, checkForUpdate } = useUpdateChecker()

    await checkForUpdate()
    expect(status.value).toBe('failed')

    await checkForUpdate()
    expect(status.value).toBe('latest')
  })
})

describe('downloadAndInstall', () => {
  it('成功下载+安装 → 状态走 downloaded → installing → relaunch', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'install_update') {
        // 安装期间 Rust 会推 finished 事件
        setTimeout(() => emitProgress({ downloaded: 100, content_length: 100, finished: true }), 0)
        return true
      }
      throw new Error(`unexpected invoke: ${cmd}`)
    })
    const { status, downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall()

    expect(mockInvoke).toHaveBeenCalledWith('install_update')
    expect(mockRelaunch).toHaveBeenCalled()
    expect(status.value).toBe('installing')
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it('订阅在 invoke 之前建立（晚订阅会丢首批进度事件）', async () => {
    const order: string[] = []
    mockInvoke.mockImplementation(async (cmd: string) => {
      order.push(cmd)
      return cmd === 'install_update'
    })
    const { downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall()

    // listen 同步把 handler 压进 progressHandlers，而它早于 install_update 的调用
    expect(progressHandlers.length).toBeGreaterThan(0)
    expect(order).toEqual(['install_update'])
  })

  it('进度事件 → 累计字节直写 + onProgress 回调 + 完成后转 downloaded', async () => {
    const onProgress = vi.fn()
    mockInvoke.mockResolvedValue(false) // 无更新：不会走到完成分支
    const { downloadProgress, downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall(onProgress)
    emitProgress({ downloaded: 512, content_length: 2048, finished: false })

    expect(downloadProgress.value).toEqual({ downloaded: 512, contentLength: 2048 })
    expect(onProgress).toHaveBeenCalledWith(512, 2048)

    emitProgress({ downloaded: 2048, content_length: 2048, finished: true })
    expect(useUpdateChecker().status.value).toBe('downloaded')
  })

  it('无可用更新（install_update 返回 false）→ 不算失败、不 relaunch', async () => {
    mockInvoke.mockResolvedValue(false)
    const { status, downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall()

    expect(status.value).toBe('available')
    expect(mockRelaunch).not.toHaveBeenCalled()
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it('下载/安装失败 → status=failed 且详情只进日志', async () => {
    const raw = new Error('checksum mismatch: expected abc123')
    mockInvoke.mockRejectedValue(raw)
    const { status, downloadAndInstall } = useUpdateChecker()

    await downloadAndInstall()

    expect(status.value).toBe('failed')
    expect(errorSpy).toHaveBeenCalledWith('[update-checker] downloadAndInstall failed:', raw)
    expect(useUpdateChecker().getUpdateStatusText()).not.toContain('checksum')
  })

  it('任何退出路径都解除事件订阅（不泄漏监听器）', async () => {
    mockInvoke.mockRejectedValue(new Error('boom'))
    await useUpdateChecker().downloadAndInstall()
    expect(mockUnlisten).toHaveBeenCalled()

    mockUnlisten.mockClear()
    mockInvoke.mockResolvedValue(true)
    mockRelaunch.mockResolvedValue(undefined)
    await useUpdateChecker().downloadAndInstall()
    expect(mockUnlisten).toHaveBeenCalled()
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