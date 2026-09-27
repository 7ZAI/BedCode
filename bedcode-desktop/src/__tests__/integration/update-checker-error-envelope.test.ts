/**
 * 更新检查全链路错误信封集成测试（票 04，票 05 启用，随全量回归执行）
 *
 * 垂直切片：tauri-plugin-updater `check()` reject（原始技术错误）
 *   → useUpdateChecker（status=failed，详情只进 logger.error）
 *   → SettingsAboutSection（设置页渲染通用文案「检查更新失败，请稍后重试」）
 *   → 断言：DOM / toast 均不含错误原文、堆栈、命令名，详情只在日志
 *
 * 断言的硬不变量（ADR 0030 §2 / 票 04 验收）：
 * - 设置页失败段落只显示 i18n 通用文案
 * - 原始错误（含触发原因、URL、堆栈片段）绝不出现在任何可渲染文本
 * - logger.error 拿到原始错误（唯一技术详情落点；release 下仍转发落盘）
 *
 * 驱动方式说明（票 05 启用时实测修正）：SettingsAboutSection **不自动触发**更新检查
 * （挂载只读版本号；「检查更新」按钮在 SettingsView 工具栏），故本文件经
 * useUpdateChecker 组合式函数驱动 `checkForUpdate()`，组件只做失败态的 DOM 断言。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import { configureLogger, resetLoggerState, logger } from '@/utils/frontendLogger'
import SettingsAboutSection from '@/components/settings/SettingsAboutSection.vue'
import { useUpdateChecker } from '@/composables/useUpdateChecker'

// ==================== mock Tauri 边界 ====================

const mockCheck = vi.fn()
vi.mock('@tauri-apps/plugin-updater', () => ({
  check: (...args: unknown[]) => mockCheck(...args),
}))

const mockRelaunch = vi.fn()
vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}))

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

const mockOpen = vi.fn()
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: (...args: unknown[]) => mockOpen(...args),
}))

vi.mock('vue-sonner', () => ({
  toast: {
    success: vi.fn(() => 'mock-id'),
    error: vi.fn(() => 'mock-id-error'),
    warning: vi.fn(() => 'mock-id'),
    info: vi.fn(() => 'mock-id'),
  },
}))

import { toast } from 'vue-sonner'
const mockedToast = vi.mocked(toast)

async function flushAsync(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0))
}

let errorSpy: ReturnType<typeof vi.spyOn>
let wrapper: ReturnType<typeof mount> | null = null

beforeEach(() => {
  vi.clearAllMocks()
  // mock 队列卫生：clearAllMocks 不清 once 队列，跨用例残留会让用例拿到
  // 前一个用例的 rejection；reset 后每用例独立设置
  mockCheck.mockReset()
  mockRelaunch.mockReset()
  mockOpen.mockReset()
  mockInvoke.mockReset()
  resetLoggerState()
  configureLogger(true)
  setActivePinia(createPinia())
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  // 挂载时读取版本号：invoke('get_app_version')
  mockInvoke.mockResolvedValue('1.0.0')
  // 载入前把单例状态复位（模块级 ref 跨用例共享）
  const { status } = useUpdateChecker()
  status.value = 'idle'
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  errorSpy.mockRestore()
  resetLoggerState()
  configureLogger(true)
})

describe('更新检查失败全链路', () => {
  it('check() 抛技术错误 → 设置页显示通用文案，原文只进日志', async () => {
    // 最真实的失败形状：updater 插件网络/签名错误，含 URL / 堆栈片段
    const raw = new Error(
      'updater check failed: GET https://example.com/latest.json -> 500 (server error)\n    at fetchUrl (updater.js:42:10)',
    )
    mockCheck.mockRejectedValueOnce(raw)

    const { checkForUpdate } = useUpdateChecker()
    wrapper = mount(SettingsAboutSection, {
      global: { plugins: [createPinia(), i18n] },
    })
    await flushAsync()
    await checkForUpdate() // 触发检查（真实入口在 SettingsView 工具栏，此处直接驱动）
    await flushAsync()

    // 设置页失败段落：i18n 通用文案
    const text = wrapper.text()
    expect(text).toContain('检查更新失败，请稍后重试')

    // 硬不变量：任何可渲染文本不含错误原文 / URL / 堆栈 / 状态码
    for (const fragment of [
      'updater check failed',
      'https://example.com',
      'latest.json',
      '500',
      'fetchUrl',
      'updater.js',
      raw.message,
      raw.stack ?? 'never-match',
    ]) {
      expect(text).not.toContain(fragment)
    }

    // 详情唯一落点：logger.error（release 下仍转发落盘，见 frontendLogger）
    expect(errorSpy).toHaveBeenCalledWith('[update-checker] checkForUpdate failed:', raw)
    expect(mockedToast.error).not.toHaveBeenCalled()
  })

  it('失败后重新检查成功 → 状态恢复为已是最新版本，失败段落消失', async () => {
    mockCheck.mockRejectedValueOnce(new Error('network is unreachable'))
    const { checkForUpdate, status } = useUpdateChecker()
    wrapper = mount(SettingsAboutSection, {
      global: { plugins: [createPinia(), i18n] },
    })
    await flushAsync()
    await checkForUpdate()
    await flushAsync()
    expect(wrapper.text()).toContain('检查更新失败，请稍后重试')

    // 第二次检查成功（无更新）→ 状态清回 latest，失败段落消失
    mockCheck.mockResolvedValueOnce(null)
    await checkForUpdate()
    await flushAsync()
    expect(status.value).toBe('latest')
    expect(wrapper.text()).not.toContain('检查更新失败')
  })

  it('下载/安装失败 → 详情只进日志，页面保持通用失败态无原文', async () => {
    const raw = new Error('checksum mismatch: expected 8f3a... got deadbeef')
    const installing = {
      downloadAndInstall: vi.fn().mockRejectedValueOnce(raw),
    }
    mockCheck.mockResolvedValue(installing) // 检查 + 下载内部 check 都命中

    const { checkForUpdate, downloadAndInstall } = useUpdateChecker()
    wrapper = mount(SettingsAboutSection, {
      global: { plugins: [createPinia(), i18n] },
    })
    await flushAsync()
    await checkForUpdate() // → available（显示下载按钮态）
    await downloadAndInstall() // → downloading → downloadAndInstall reject → failed
    await flushAsync()

    const text = wrapper.text()
    expect(text).toContain('检查更新失败，请稍后重试')
    expect(text).not.toContain('checksum mismatch')
    expect(text).not.toContain('8f3a')
    expect(errorSpy).toHaveBeenCalledWith('[update-checker] downloadAndInstall failed:', raw)
  })

  it('GitHub 打开失败 → 只记日志，不弹任何技术错误 toast', async () => {
    const raw = new Error('failed to open external link: protocol handler missing')
    mockOpen.mockRejectedValueOnce(raw)

    wrapper = mount(SettingsAboutSection, {
      global: { plugins: [createPinia(), i18n] },
    })
    await flushAsync()
    // 无检查失败场景时不渲染失败段
    const buttons = wrapper.findAll('button')
    const ghBtn = buttons.find((b) => b.text().includes('GitHub 仓库'))
    expect(ghBtn).toBeTruthy()
    await ghBtn!.trigger('click')
    await flushAsync()

    expect(errorSpy).toHaveBeenCalledWith('Failed to open GitHub repo:', raw)
    expect(mockedToast.error).not.toHaveBeenCalled()
    expect(wrapper.text()).not.toContain('protocol handler')
  })
})