/**
 * 插件系统 → 宿主壳 适配器 行为契约测试
 *
 * 覆盖：插件运行时状态到壳运行态的映射、权限授予口径、运行面解析优先级、
 *       启动/停止的分形态分流，以及「不支持的能力返回 false」这一诚实口径。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, h } from 'vue'

// ==================== 替身（只替跨进程/第三方边界） ====================
vi.mock('@/plugin/commands', () => ({
  pluginListLoaded: vi.fn(async () => []),
  pluginGetInfo: vi.fn(async () => null),
  pluginPreauthorize: vi.fn(async () => {}),
  pluginActivate: vi.fn(async () => {}),
  pluginDeactivate: vi.fn(async () => {}),
  pluginInstallFromFile: vi.fn(async () => 'installed-id'),
  pluginUninstall: vi.fn(async () => {}),
}))

vi.mock('@/plugin/loader', () => ({
  pluginLoader: {
    activate: vi.fn(async () => {}),
    deactivate: vi.fn(async () => {}),
    getActivePlugin: vi.fn(() => undefined),
  },
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(async () => null),
  confirm: vi.fn(async () => false),
}))

import { open as dialogOpen } from '@tauri-apps/plugin-dialog'
import * as cmds from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import { getPluginRegistry } from '@/plugin/registry'
import { getShellRegistry } from '@/shell/registry'
import type { PluginInfo, PluginState } from '@/plugin/types'
import { createPluginAppSource, PLUGIN_APP_SOURCE_ID, toShellApp } from '@/shell/adapters/pluginAppSource'

/** 最小插件信息 */
function info(state: PluginState, patch: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: 'com.test.app',
    name: '测试应用',
    version: '1.2.3',
    description: '',
    author: '',
    main: 'index.js',
    pluginType: 'rust-ts',
    permissions: [],
    contributes: {},
    state,
    source: 'apk-asset',
    extensionPath: '/tmp/app',
    sizeBytes: 1024,
    ...patch,
  } as PluginInfo
}

const registry = getPluginRegistry()
const shellRegistry = getShellRegistry()

beforeEach(() => {
  vi.clearAllMocks()
  registry.clearPlugin('com.test.app')
  shellRegistry.clearApp('com.test.app')
})

describe('插件状态 → 壳运行态', () => {
  it('should_beRunning_when_pluginActivated', () => {
    expect(toShellApp(info({ state: 'Activated' })).state).toBe('running')
  })

  it('should_beRunning_when_pluginDegraded', () => {
    // 降级但实例存活、命令可用；显示成「已停止」会让用户误以为没起来
    expect(toShellApp(info({ state: 'Degraded', error: 'slow' })).state).toBe('running')
  })

  it('should_beDisabled_when_pluginNeedsApproval', () => {
    expect(toShellApp(info({ state: 'NeedsApproval' })).state).toBe('disabled')
  })

  it('should_beErrorWithReason_when_pluginErrored', () => {
    const app = toShellApp(info({ state: 'Error', error: 'boom' }))
    expect(app.state).toBe('error')
    expect(app.error).toBe('boom')
  })

  it('should_beStopped_when_pluginLoadedOrActivating', () => {
    expect(toShellApp(info({ state: 'Loaded' })).state).toBe('stopped')
    expect(toShellApp(info({ state: 'Activating' })).state).toBe('stopped')
  })

  it('should_beDisabled_when_pluginDeactivated', () => {
    expect(toShellApp(info({ state: 'Deactivated' })).state).toBe('disabled')
  })
})

describe('权限映射', () => {
  it('should_markGrantedAndLockStorage_when_approved', () => {
    const app = toShellApp(
      info({ state: 'Activated' }, { permissions: ['storage', 'fs:write'] }),
    )

    expect(app.permissions.map((p) => p.granted)).toEqual([true, true])
    expect(app.permissions.find((p) => p.key === 'storage')?.locked).toBe(true)
    expect(app.permissions.find((p) => p.key === 'fs:write')?.locked).toBe(false)
  })

  it('should_markNotGranted_when_pluginNeedsApproval', () => {
    const app = toShellApp(info({ state: 'NeedsApproval' }, { permissions: ['fs:read'] }))
    // 未人工批准 ⇒ 一律未授予（不编造逐项授予状态：后端没有暴露它）
    expect(app.permissions[0].granted).toBe(false)
  })

  it('should_beOfficial_when_packagedInApkAssets', () => {
    expect(toShellApp(info({ state: 'Loaded' })).official).toBe(true)
    expect(toShellApp(info({ state: 'Loaded' }, { source: 'user' })).official).toBe(false)
  })
})

describe('运行面解析（票 2026-10-10 C1：只认 registerSurface）', () => {
  it('should_returnUndefined_when_appRegisteredNoSurface', () => {
    const surface = createPluginAppSource()
    expect(surface.resolveSurface?.('com.test.app')).toBeUndefined()
  })

  it('should_returnSurfaceComponent_when_appRegisteredSurface', () => {
    const component = defineComponent({ name: 'AppSurface', setup: () => () => h('div') })
    // 生产顺序：list() 先把应用写进壳注册表，激活后才 registerSurface
    shellRegistry.upsertApps(PLUGIN_APP_SOURCE_ID, [toShellApp(info({ state: 'Activated' }))])
    shellRegistry.registerSurface('com.test.app', { component })

    expect(createPluginAppSource().resolveSurface?.('com.test.app')).toBe(component)
  })

  it('should_returnUndefined_when_appNotInRegistryAtAll', () => {
    // 反例面：应用从未被 list() 写入壳注册表（未安装 / 已卸载）时不得凭空解析出运行面
    expect(createPluginAppSource().resolveSurface?.('com.not.installed')).toBeUndefined()
  })

  it('should_notRequireSurface_when_appRegisteredOnlyLegacyUi', () => {
    // 反例面（退役锁）：旧嵌入扩展点（工具箱页 / 终端主视图）的注册面已整面退役，
    // 应用即使注册了它们也不得被壳当成整个应用的运行面——这里只能有 surface 一条路。
    // 退役扩展点本身的存在性由 context.ts 的显性抛错与 R4 退役锁钉住。
    shellRegistry.upsertApps(PLUGIN_APP_SOURCE_ID, [toShellApp(info({ state: 'Activated' }))])

    expect(createPluginAppSource().resolveSurface?.('com.test.app')).toBeUndefined()
  })
})

describe('启动与停止', () => {
  it('should_preauthorizeThenLoadFrontend_when_pluginHasFrontend', async () => {
    vi.mocked(cmds.pluginGetInfo).mockResolvedValueOnce(info({ state: 'Loaded' }))
    const source = createPluginAppSource()

    await source.launch('com.test.app')

    expect(cmds.pluginPreauthorize).toHaveBeenCalledWith('com.test.app')
    expect(pluginLoader.activate).toHaveBeenCalledWith('com.test.app')
    // 带前端模块的插件不能走裸 pluginActivate，否则扩展点不会注册
    expect(cmds.pluginActivate).not.toHaveBeenCalled()
  })

  it('should_activateBackendOnly_when_pluginIsRustOnly', async () => {
    vi.mocked(cmds.pluginGetInfo).mockResolvedValueOnce(
      info({ state: 'Loaded' }, { pluginType: 'rust' }),
    )
    const source = createPluginAppSource()

    await source.launch('com.test.app')

    // rust-only 没有前端入口，loader 会尝试 import 不存在的入口而失败
    expect(cmds.pluginActivate).toHaveBeenCalledWith('com.test.app')
    expect(pluginLoader.activate).not.toHaveBeenCalled()
  })

  it('should_deactivateFrontendAndBackend_when_stopCalled', async () => {
    const source = createPluginAppSource()
    await source.stop('com.test.app')

    expect(pluginLoader.deactivate).toHaveBeenCalledWith('com.test.app')
    // 前端从未加载过的插件（如 rust-only）loader 无记录，仍需通知后端
    expect(cmds.pluginDeactivate).toHaveBeenCalledWith('com.test.app')
  })
})

describe('能力诚实口径', () => {
  it('should_returnFalse_when_permissionGrantUnsupported', async () => {
    const source = createPluginAppSource()
    // 后端暂无逐项授权命令：返回 false 让 UI 置灰并说明，而不是假装通过
    await expect(source.setPermissionGrant?.('com.test.app', 'fs:read', true)).resolves.toBe(false)
  })

  it('should_returnFalseWithoutInstalling_when_userCancelsFilePicker', async () => {
    vi.mocked(dialogOpen).mockResolvedValueOnce(null as unknown as string)
    const source = createPluginAppSource()

    await expect(source.installFromLocalPackage?.()).resolves.toBe(false)
    expect(cmds.pluginInstallFromFile).not.toHaveBeenCalled()
  })

  it('should_installChosenPackage_when_userPicksFile', async () => {
    vi.mocked(dialogOpen).mockResolvedValueOnce('/sdcard/app.zip')
    const source = createPluginAppSource()

    await expect(source.installFromLocalPackage?.()).resolves.toBe(true)
    expect(cmds.pluginInstallFromFile).toHaveBeenCalledWith('/sdcard/app.zip')
  })

  it('should_reportSourceId_when_created', () => {
    expect(createPluginAppSource().id).toBe(PLUGIN_APP_SOURCE_ID)
  })
})
