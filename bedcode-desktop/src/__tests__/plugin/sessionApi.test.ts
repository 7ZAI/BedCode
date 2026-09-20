/**
 * PluginContext.session 终端窗口原语行为契约（票 13）
 *
 * 终端窗口本体、创建尺寸与渲染管线留宿主（spec D3）；插件会话页只经
 * `context.session` 触发。被测契约（外部可见行为）：
 * - C1 权限门：无 `session:read` 即快速失败（三方法各自报自己的 api 名）
 * - C2 openTerminal 委派宿主窗口管理器并回传「是否新建」（调用方据此显示 loading）
 * - C3 closeTerminal 委派宿主窗口管理器（幂等语义由宿主保证）
 * - C4 predictTerminalSize 以宿主设置字体大小 + 窗口创建比例调用宿主预测
 * - C5 预测不可用时原样返回 null（调用方不传尺寸，宿主兜底默认网格）
 *
 * 不测内部实现：断言的是「宿主既有能力被以正确的入参调用」与权限边界。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { createPluginContext } from '@/plugin/context'
import { useSettingsStore } from '@/stores/settings'
import type { PluginInfo } from '@/plugin/types'

const openTerminalWindow = vi.fn(async () => true)
const closeTerminalWindow = vi.fn(async () => {})
const hasTerminalWindow = vi.fn(() => false)
vi.mock('@/composables/useSessionWindows', () => ({
  useSessionWindows: () => ({ openTerminalWindow, closeTerminalWindow, hasTerminalWindow }),
}))

const computeDesktopInitialTerminalSize = vi.fn(async () => ({ cols: 120, rows: 30 }))
vi.mock('@/utils/terminalInitialSize', () => ({
  TERMINAL_WINDOW_WIDTH_RATIO: 0.6,
  computeDesktopInitialTerminalSize: (...args: unknown[]) =>
    computeDesktopInitialTerminalSize(...(args as [])),
}))

function makeContext(permissions: string[]) {
  const info = {
    id: 'com.test.plugin',
    name: 'Test',
    version: '1.0.0',
    description: '',
    author: '',
    main: 'index.js',
    sandbox: 'inline',
    pluginType: 'ts-only',
    permissions,
    state: { state: 'Activated' },
    extensionPath: '/tmp/plugin',
    contributes: {},
    source: 'builtin',
    sizeBytes: 0,
  } as unknown as PluginInfo
  return createPluginContext(info)
}

describe('PluginContext.session 终端窗口原语', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setActivePinia(createPinia())
    computeDesktopInitialTerminalSize.mockResolvedValue({ cols: 120, rows: 30 } as never)
    openTerminalWindow.mockResolvedValue(true)
    hasTerminalWindow.mockReturnValue(false)
  })

  it('C1 无 session:read 权限时四个方法均快速失败并指名 api', async () => {
    const ctx = makeContext([])

    await expect(ctx.session.predictTerminalSize()).rejects.toThrow(
      'lacks permission for session.predictTerminalSize',
    )
    await expect(ctx.session.openTerminal({ id: 's-1', name: 'dev' })).rejects.toThrow(
      'lacks permission for session.openTerminal',
    )
    await expect(ctx.session.closeTerminal('s-1')).rejects.toThrow(
      'lacks permission for session.closeTerminal',
    )
    expect(() => ctx.session.isTerminalOpen('s-1')).toThrow(
      'lacks permission for session.isTerminalOpen',
    )
    // 权限门在触达宿主能力前生效
    expect(openTerminalWindow).not.toHaveBeenCalled()
    expect(hasTerminalWindow).not.toHaveBeenCalled()
  })

  it('C2 openTerminal 委派宿主窗口管理器并回传是否新建窗口', async () => {
    const ctx = makeContext(['session:read'])

    await expect(ctx.session.openTerminal({ id: 's-1', name: 'dev' })).resolves.toBe(true)
    expect(openTerminalWindow).toHaveBeenCalledWith({ id: 's-1', name: 'dev' })

    // 既有窗口聚焦：宿主返回 false（插件页据此不显示 loading）
    openTerminalWindow.mockResolvedValue(false)
    await expect(ctx.session.openTerminal({ id: 's-1', name: 'dev' })).resolves.toBe(false)
  })

  it('C3 closeTerminal 委派宿主窗口管理器（参数为会话 id）', async () => {
    const ctx = makeContext(['session:read'])

    await ctx.session.closeTerminal('s-9')
    expect(closeTerminalWindow).toHaveBeenCalledWith('s-9')
  })

  it('C4 predictTerminalSize 以宿主设置字体大小 + 窗口创建比例调用宿主预测', async () => {
    const settingsStore = useSettingsStore()
    settingsStore.settings.ui.terminal_font_size = 15
    const ctx = makeContext(['session:read'])

    await expect(ctx.session.predictTerminalSize()).resolves.toEqual({ cols: 120, rows: 30 })
    expect(computeDesktopInitialTerminalSize).toHaveBeenCalledWith(15, { widthRatio: 0.6 })
  })

  it('C5 预测不可用（null）时原样返回，由宿主兜底默认网格', async () => {
    computeDesktopInitialTerminalSize.mockResolvedValue(null as never)
    const ctx = makeContext(['session:read'])

    await expect(ctx.session.predictTerminalSize()).resolves.toBeNull()
  })

  it('C6 isTerminalOpen 同步反映宿主窗口登记事实（决定是否显示就绪 loading）', () => {
    const ctx = makeContext(['session:read'])

    hasTerminalWindow.mockReturnValue(false)
    expect(ctx.session.isTerminalOpen('s-1')).toBe(false)

    hasTerminalWindow.mockReturnValue(true)
    expect(ctx.session.isTerminalOpen('s-1')).toBe(true)
    expect(hasTerminalWindow).toHaveBeenCalledWith('s-1')
  })
})
