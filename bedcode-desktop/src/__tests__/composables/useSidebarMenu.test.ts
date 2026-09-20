import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import {
  useSidebarMenu,
  registerSidebarItem,
  builtinMenuItems,
} from '@/composables/useSidebarMenu'
import { getPluginRegistry } from '@/plugin/registry'

/**
 * useSidebarMenu 测试 — 统一菜单合并与顺序扩展点
 *
 * 会话 / 设备配对内置入口已随票 13/14 下沉 com.bedcode.session 插件，宿主菜单
 * 仅剩插件管理与设置两个恒最末入口；插件贡献目录按自身 order 参与统一排序，
 * 且仅运行态插件（Activated / Degraded）的目录可见（error / 停用即摘除，D7）。
 */
describe('useSidebarMenu', () => {
  const registry = getPluginRegistry()
  const disposables: Array<{ dispose: () => void }> = []
  const usedPluginIds = new Set<string>()

  /** 注册一个运行态插件的贡献目录（sidebar 或 toolbox），返回菜单项 id */
  function registerPluginView(
    pluginId: string,
    viewId: string,
    viewType: string,
    order?: number,
  ): string {
    registry.setPluginState(pluginId, { state: 'Activated' })
    usedPluginIds.add(pluginId)
    const d = registry.registerView(pluginId, viewType, {
      id: viewId,
      title: `${pluginId} ${viewId}`,
      icon: 'M1 1h4',
      order,
      component: {},
    })
    disposables.push(d)
    return `plugin-${pluginId}-${viewId}`
  }

  beforeEach(() => {
    disposables.length = 0
  })

  afterEach(() => {
    // 清理插件视图与自定义菜单项，避免单例注册表污染后续用例
    for (const d of disposables.splice(0)) {
      d.dispose()
    }
    for (const id of usedPluginIds) {
      registry.clearPlugin(id)
    }
    usedPluginIds.clear()
  })

  it('默认只有内置菜单项（插件管理与设置），按 order 升序排列', () => {
    const { menuItems } = useSidebarMenu()
    expect(menuItems.value.map((m) => m.id)).toEqual(['plugins', 'settings'])
  })

  it('插件面板与内置菜单合并为单一列表，未指定 order 时排在设置/插件管理之前', () => {
    registerPluginView('p1', 'v1', 'sidebar')
    registerPluginView('p2', 'v2', 'toolbox')

    const { menuItems } = useSidebarMenu()
    const ids = menuItems.value.map((m) => m.id)
    // 插件默认 order 600：位于内置业务菜单之后，但始终排在插件管理(9998)/设置(9999)之前
    expect(ids).toEqual(['plugin-p1-v1', 'plugin-p2-v2', 'plugins', 'settings'])
  })

  it('插件可通过 order 插入到任意位置，插队只是排序不是接管', () => {
    // order 150：位于两个插件目录之间；order 500：位于默认插件目录(600)之前
    registerPluginView('p1', 'v1', 'sidebar', 150)
    registerPluginView('p2', 'v2', 'toolbox', 500)

    const { menuItems } = useSidebarMenu()
    expect(menuItems.value.map((m) => m.id)).toEqual([
      'plugin-p1-v1',
      'plugin-p2-v2',
      'plugins',
      'settings',
    ])
  })

  it('sidebar 与 toolbox 视图生成正确的路由路径', () => {
    registerPluginView('p1', 'v1', 'sidebar')
    registerPluginView('p2', 'v2', 'toolbox')

    const { menuItems } = useSidebarMenu()
    const sidebar = menuItems.value.find((m) => m.id === 'plugin-p1-v1')!
    const toolbox = menuItems.value.find((m) => m.id === 'plugin-p2-v2')!

    expect(sidebar.path).toBe('/plugin/sidebar/p1/v1')
    expect(sidebar.prefix).toBe(true)
    expect(toolbox.path).toBe('/plugin/toolbox/p2/v2')
    expect(toolbox.prefix).toBe(true)
  })

  it('registerSidebarItem 扩展点可按 order 插入菜单，dispose 后移除', () => {
    const custom = registerSidebarItem({
      id: 'custom',
      path: '/custom',
      labelKey: 'Custom',
      order: 250,
    })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    expect(menuItems.value.map((m) => m.id)).toEqual(['custom', 'plugins', 'settings'])

    // dispose 后菜单项移除
    custom.dispose()
    expect(menuItems.value.map((m) => m.id)).toEqual(['plugins', 'settings'])
  })

  it('自定义项支持 i18n key 与纯文本标题标记', () => {
    const custom = registerSidebarItem({
      id: 'i18n-item',
      path: '/x',
      labelKey: 'desktop.sidebar.session',
      isI18nKey: true,
      order: 1,
    })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    const item = menuItems.value.find((m) => m.id === 'i18n-item')!
    expect(item.isI18nKey).toBe(true)
    expect(item.labelKey).toBe('desktop.sidebar.session')
  })

  it('同 order 时保持 内置 → 自定义 → 插件 的稳定顺序', () => {
    registerPluginView('p1', 'v1', 'sidebar', 9998)
    const custom = registerSidebarItem({
      id: 'custom',
      path: '/custom',
      labelKey: 'Custom',
      order: 9998,
    })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    const ids = menuItems.value.map((m) => m.id)
    // 与"插件管理"同 order=9998：内置在前，自定义次之，插件最后
    expect(ids.indexOf('plugins')).toBeLessThan(ids.indexOf('custom'))
    expect(ids.indexOf('custom')).toBeLessThan(ids.indexOf('plugin-p1-v1'))
  })

  // ==================== 贡献目录按运行态过滤（票 18 / spec D7） ====================

  describe('贡献目录随插件运行态摘除与恢复', () => {
    const MERGED = 'com.bedcode.session'

    it('运行态插件（Activated）的贡献目录可见，error 后整组摘除、恢复后整组回来', () => {
      const panelIds = [
        ['session.pairing', 100],
        ['session.history', 101],
        ['session.sidebar', 200],
        ['session.task-history', 210],
      ].map(([viewId, order]) => registerPluginView(MERGED, viewId, 'sidebar', order))

      const ids = () => useSidebarMenu().menuItems.value.map((m) => m.id)

      // 四目录按 order 落在原位，末尾两项恒在最末
      expect(ids()).toEqual([...panelIds, 'plugins', 'settings'])

      // error 态：四目录整组摘除，不残留空目录
      usedPluginIds.add(MERGED)
      registry.setPluginState(MERGED, { state: 'Error', error: 'wasm trap in task domain' })
      expect(ids()).toEqual(['plugins', 'settings'])

      // 恢复激活：整组回来
      registry.setPluginState(MERGED, { state: 'Activated' })
      expect(ids()).toEqual([...panelIds, 'plugins', 'settings'])
    })

    it('未登记运行态的插件（激活未完成）其贡献目录不出现', () => {
      usedPluginIds.add('p-activating')
      disposables.push(
        registry.registerView('p-activating', 'sidebar', {
          id: 'v',
          title: 'activating',
          order: 100,
          component: {},
        }),
      )
      registry.setPluginState('p-activating', { state: 'Activating' })

      const ids = () => useSidebarMenu().menuItems.value.map((m) => m.id)
      expect(ids()).toEqual(['plugins', 'settings'])
    })

    it('插件停用后贡献目录摘除', () => {
      registerPluginView(MERGED, 'pairing', 'sidebar', 100)
      const ids = () => useSidebarMenu().menuItems.value.map((m) => m.id)
      expect(ids()).toContain('plugin-com.bedcode.session-pairing')

      registry.clearPlugin(MERGED)
      expect(ids()).toEqual(['plugins', 'settings'])
    })
  })

  it('内置菜单恒为插件管理与设置两项，order 恒最末', () => {
    expect(builtinMenuItems.map((i) => i.id)).toEqual(['plugins', 'settings'])
    expect(builtinMenuItems.every((i) => i.order >= 9998)).toBe(true)
  })
})
