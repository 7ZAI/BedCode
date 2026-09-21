/**
 * 懒激活成功路径的贡献面生效测试（票 02）
 *
 * 被测行为：用户从插件管理页启用插件、或经深链触发 `pluginLoader.activate(id)` 后，
 * 该插件在 activate() 里注册的侧边栏目录立即生效，并据此让宿主同域内置入口让位。
 *
 * 为什么单独测：loader 的 `manifest` 是激活**前**取的快照（state 仍是 Loaded），
 * 若把该快照直接登记为运行态，贡献面会被判「未生效」——菜单不摘、设置分组不渲染，
 * 且插件管理页之外的路径（深链懒激活）最容易踩到。
 *
 * 测试 seam（与 plugin-loader-gating.test.ts 同模式）：只 mock @tauri-apps/api 边界。
 * 与门禁测试不同，这里让入口模块**导入成功**（data: URL 是合法 ESM），
 * 从而走到 loader 的成功分支；断言外部可见结果 = 菜单项列表形态。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { logger } from '@/utils/frontendLogger'
import { pluginLoader } from '@/plugin/loader'
import { getPluginRegistry } from '@/plugin/registry'
import { useSidebarMenu } from '@/composables/useSidebarMenu'
import { makePluginInfo } from '@/__tests__/fixtures/index'

const mockInvoke = vi.fn()

/** 插件入口模块源码：activate 时注册一个占用「设备配对」槽位（order 100）的目录 */
const ENTRY_SOURCE = `
export async function activate(ctx) {
  ctx.ui.registerSidebarPanel({
    id: 'pairing',
    title: '设备与配对',
    icon: 'M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z',
    order: 100,
    component: { template: '<div />' },
  })
}
`

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  // 测试环境无 asset 服务器：改吐 data: URL，让动态 import 真实成功
  convertFileSrc: () => `data:text/javascript,${encodeURIComponent(ENTRY_SOURCE)}`,
}))

const PLUGIN_ID = 'com.bedcode.terminal-session'

function installInvokeMock(staleManifest: ReturnType<typeof makePluginInfo>) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'plugin_get_info') return Promise.resolve(staleManifest)
    return Promise.resolve(undefined)
  })
}

/** 当前菜单项 id 列表 */
function menuIds(): string[] {
  return useSidebarMenu().menuItems.value.map((m) => m.id)
}

describe('pluginLoader.activate 懒激活成功后贡献面生效', () => {
  const registry = getPluginRegistry()
  /** 后端在激活前返回的清单快照：state 仍是 Loaded（真实时序） */
  const staleManifest = makePluginInfo({
    id: PLUGIN_ID,
    state: { state: 'Loaded' },
    permissions: ['ui:sidebar'],
  })

  let consoleErrorSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    vi.clearAllMocks()
    installInvokeMock(staleManifest)
    consoleErrorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  })

  afterEach(async () => {
    // loader 单例跨用例清理：停用会清注册表 + 摘除运行态
    await pluginLoader.deactivate(PLUGIN_ID)
    registry.clearPlugin(PLUGIN_ID)
    consoleErrorSpy.mockRestore()
  })

  it('登记激活后运行态而非激活前的快照，贡献目录随即可见', async () => {
    expect(registry.isContributionActive(PLUGIN_ID)).toBe(false)

    await pluginLoader.activate(PLUGIN_ID)

    expect(registry.getPluginState(PLUGIN_ID)).toEqual({ state: 'Activated' })
    expect(registry.sidebarViews.value.map((v) => `${v.pluginId}:${v.viewId}`)).toEqual([
      `${PLUGIN_ID}:pairing`,
    ])
  })

  it('懒激活的插件激活后贡献目录可见（宿主无内置设备入口，无需让位）', async () => {
    await pluginLoader.activate(PLUGIN_ID)

    const ids = menuIds()
    expect(ids).toContain('plugin-com.bedcode.terminal-session-pairing')
    // 票 13/14 收尾：宿主内置设备入口已删除，不再有同域双入口问题
    expect(ids).not.toContain('devices')
  })

  it('激活成功后不上报 mark_error（成功分支不得走失败恢复）', async () => {
    await pluginLoader.activate(PLUGIN_ID)

    const markErrorCalls = mockInvoke.mock.calls.filter(([c]) => c === 'plugin_mark_error')
    expect(markErrorCalls).toEqual([])
  })

  it('插件停用后贡献目录摘除，宿主菜单回落到默认（插件管理与设置）', async () => {
    await pluginLoader.activate(PLUGIN_ID)
    expect(menuIds()).toContain('plugin-com.bedcode.terminal-session-pairing')

    await pluginLoader.deactivate(PLUGIN_ID)

    // 宿主不再恢复设备入口（入口已删除），只保留恒最末两项
    expect(menuIds()).toEqual(['plugins', 'settings'])
  })
})
