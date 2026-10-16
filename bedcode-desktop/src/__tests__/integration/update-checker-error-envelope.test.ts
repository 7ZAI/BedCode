/**
 * 更新检查全链路错误信封集成测试（票 04，票 05 启用，随全量回归执行）
 *
 * 垂直切片：宿主命令 `check_for_update` reject（原始技术错误）
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
 *
 * 传输面（前端零资源访问红线）：升级检查与安装的发起权在 Rust 端，前端只 invoke
 * 宿主命令；`@tauri-apps/plugin-updater` 的调用面已随 `updater:default` 权限撤除，
 * GitHub 链接同理改走 `open_external_url`（原 `plugin-shell.open`）。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import { configureLogger, resetLoggerState, logger } from '@/utils/frontendLogger'
import SettingsAboutSection from '@/components/settings/SettingsAboutSection.vue'
import { useUpdateChecker } from '@/composables/useUpdateChecker'

// ==================== mock Tauri 边界 ====================

const mockRelaunch = vi.fn()
vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}))

const mockInvoke = vi.fn()
// 按命令名路由的 invoke handler 表；**未登记的命令直接抛错**——避免用例没跟上迁移
// 却因为 mock 返回 undefined 而静默变绿（假阴性）。
const invokeHandlers = new Map<string, (args: unknown) => Promise<unknown>>()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
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
  mockRelaunch.mockReset()
  mockInvoke.mockReset()
  invokeHandlers.clear()
  // 按命令名路由：未登记即抛错（宁可红也不静默绿）
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    const handler = invokeHandlers.get(cmd)
    if (!handler) throw new Error(`unmocked invoke command: ${cmd}`)
    return handler(args)
  })
  resetLoggerState()
  configureLogger(true)
  setActivePinia(createPinia())
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  // 挂载时读取版本号
  invokeHandlers.set('get_app_version', async () => '1.0.0')
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
  it('宿主命令抛技术错误 → 设置页显示通用文案，原文只进日志', async () => {
    // 最真实的失败形状：Rust 侧出站失败，含 URL / 堆栈片段
    const raw = new Error(
      'updater check failed: GET https://example.com/latest.json -> 500 (server error)\n    at fetchUrl (updater.js:42:10)',
    )
    invokeHandlers.set('check_for_update', async () => {
      throw raw
    })

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
    let attempts = 0
    invokeHandlers.set('check_for_update', async () => {
      attempts += 1
      if (attempts === 1) throw new Error('network is unreachable')
      return { available: false, version: null, date: null }
    })
    const { checkForUpdate, status } = useUpdateChecker()
    wrapper = mount(SettingsAboutSection, {
      global: { plugins: [createPinia(), i18n] },
    })
    await flushAsync()
    await checkForUpdate()
    await flushAsync()
    expect(wrapper.text()).toContain('检查更新失败，请稍后重试')

    // 第二次检查成功（无更新）→ 状态清回 latest，失败段落消失
    await checkForUpdate()
    await flushAsync()
    expect(status.value).toBe('latest')
    expect(wrapper.text()).not.toContain('检查更新失败')
  })

  it('下载/安装失败 → 详情只进日志，页面保持通用失败态无原文', async () => {
    const raw = new Error('checksum mismatch: expected 8f3a... got deadbeef')
    invokeHandlers.set('check_for_update', async () => ({
      available: true,
      version: '9.9.9',
      date: null,
    }))
    invokeHandlers.set('install_update', async () => {
      throw raw
    })

    const { checkForUpdate, downloadAndInstall } = useUpdateChecker()
    wrapper = mount(SettingsAboutSection, {
      global: { plugins: [createPinia(), i18n] },
    })
    await flushAsync()
    await checkForUpdate() // → available（显示下载按钮态）
    await downloadAndInstall() // → downloading → install_update reject → failed
    await flushAsync()

    const text = wrapper.text()
    expect(text).toContain('检查更新失败，请稍后重试')
    expect(text).not.toContain('checksum mismatch')
    expect(text).not.toContain('8f3a')
    expect(errorSpy).toHaveBeenCalledWith('[update-checker] downloadAndInstall failed:', raw)
  })

  // 组件层锁「已是最新版本」toast：调用方靠 `!r && status==='latest'` 分支，
  // 而 checkForUpdate 在无更新时若返回真值对象，该分支永不成立 → 点击静默无反应
  it('GitHub 打开失败 → 只记日志，不弹任何技术错误 toast', async () => {
    const raw = new Error('failed to open external link: protocol handler missing')
    invokeHandlers.set('open_external_url', async () => {
      throw raw
    })

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