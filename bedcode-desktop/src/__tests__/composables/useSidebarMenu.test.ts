import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import {
  useSidebarMenu,
  registerSidebarItem,
  builtinSupersededBy,
  builtinMenuItems,
  BUILTIN_MENU_ORDERS,
  type ContributionViewRef,
} from '@/composables/useSidebarMenu'
import { getPluginRegistry } from '@/plugin/registry'
import type { PluginState } from '@/plugin/types'

/**
 * useSidebarMenu 测试 — 统一菜单合并与顺序扩展点
 *
 * 覆盖：内置项与插件视图合并、插件 order 插入内置项之间、registerSidebarItem 扩展点、
 * 内置入口按「该域贡献插件是否处于运行态」让位与恢复（票 02）
 */
describe('useSidebarMenu', () => {
  const registry = getPluginRegistry()
  const disposables: { dispose: () => void }[] = []
  /** 本用例登记过运行态的插件，afterEach 统一清理（注册表为进程级单例） */
  const usedPluginIds = new Set<string>()

  /** 注册一个插件视图（sidebar 或 toolbox），返回其菜单项 id
   *
   * 同步登记 Activated 运行态 —— 与 loader 的加载成功路径一致：
   * 贡献面只有在其宿主插件处于运行态时才生效（见 isContributionActiveState） */
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

  it('默认只有内置菜单项，按内置 order 升序排列，设备配对位于首位、设置位于最末位', () => {
    const { menuItems } = useSidebarMenu()
    expect(menuItems.value.map((m) => m.id)).toEqual(['devices', 'sessions', 'plugins', 'settings'])
    // 设备配对菜单项使用"设备配对" i18n key
    expect(menuItems.value[0].labelKey).toBe('desktop.sidebar.devicePairing')
    expect(menuItems.value[0].isI18nKey).toBe(true)
  })

  it('插件面板与内置菜单合并为单一列表，未指定 order 时排在设置/插件管理之前', () => {
    registerPluginView('p1', 'v1', 'sidebar')
    registerPluginView('p2', 'v2', 'toolbox')

    const { menuItems } = useSidebarMenu()
    const ids = menuItems.value.map((m) => m.id)
    // 插件默认 order 600：位于内置业务菜单（sessions 200）之后，但始终排在插件管理(9998)/设置(9999)之前
    expect(ids).toEqual([
      'devices',
      'sessions',
      'plugin-p1-v1',
      'plugin-p2-v2',
      'plugins',
      'settings',
    ])
  })

  it('插件可通过 order 插入到任意内置菜单项之间', () => {
    // order 150：位于"设备配对"(100) 与"终端会话"(200) 之间——同时是「插队不等于接管该域」
    // 的守卫：非同一 order 槽位的贡献不得让内置设备入口让位
    registerPluginView('p1', 'v1', 'sidebar', 150)
    // order 350：位于"终端会话"(200) 之后（server 槽位 300 已保留不复用）
    registerPluginView('p2', 'v2', 'toolbox', 350)

    const { menuItems } = useSidebarMenu()
    expect(menuItems.value.map((m) => m.id)).toEqual([
      'devices',
      'plugin-p1-v1',
      'sessions',
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
    expect(menuItems.value.map((m) => m.id)).toEqual([
      'devices',
      'sessions',
      'custom',
      'plugins',
      'settings',
    ])

    // dispose 后菜单项移除
    custom.dispose()
    expect(menuItems.value.map((m) => m.id)).toEqual(['devices', 'sessions', 'plugins', 'settings'])
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
    registerPluginView('p1', 'v1', 'sidebar', 100)
    const custom = registerSidebarItem({
      id: 'custom',
      path: '/custom',
      labelKey: 'Custom',
      order: 100,
    })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    const ids = menuItems.value.map((m) => m.id)
    // 与"设备配对"同 order=100：内置在前，自定义次之，插件最后
    expect(ids.indexOf('devices')).toBeLessThan(ids.indexOf('custom'))
    expect(ids.indexOf('custom')).toBeLessThan(ids.indexOf('plugin-p1-v1'))
  })

  // ==================== 内置入口让位与恢复（票 02） ====================

  describe('内置入口按贡献插件运行态让位', () => {
    /** 取当前菜单项 id 列表 */
    function ids(): string[] {
      return useSidebarMenu().menuItems.value.map((m) => m.id)
    }

    it('贡献目录落在「设备配对」槽位时内置入口让位，同域只剩一个入口', () => {
      registerPluginView('com.bedcode.session', 'pairing', 'sidebar', BUILTIN_MENU_ORDERS.devices)

      expect(ids()).toEqual([
        'plugin-com.bedcode.session-pairing',
        'sessions',
        'plugins',
        'settings',
      ])
    })

    it('插件停用后贡献目录摘除、内置入口恢复未激活形态', () => {
      registerPluginView('com.bedcode.session', 'pairing', 'sidebar', BUILTIN_MENU_ORDERS.devices)
      expect(ids()).not.toContain('devices')

      registry.clearPlugin('com.bedcode.session')
      expect(ids()).toEqual(['devices', 'sessions', 'plugins', 'settings'])
    })

    it('插件进入 error 态后贡献目录随之摘除且内置入口恢复，不残留空目录也不出现两个入口', () => {
      registerPluginView('com.bedcode.session', 'pairing', 'sidebar', BUILTIN_MENU_ORDERS.devices)
      usedPluginIds.add('com.bedcode.session')

      registry.setPluginState('com.bedcode.session', { state: 'Error', error: 'wasm trap' })
      expect(ids()).not.toContain('plugin-com.bedcode.session-pairing')
      expect(ids()).toContain('devices')

      // 恢复激活：贡献目录回来、内置入口再次让位（摘除与恢复共用同一判据）
      registry.setPluginState('com.bedcode.session', { state: 'Activated' })
      expect(ids()).toContain('plugin-com.bedcode.session-pairing')
      expect(ids()).not.toContain('devices')
    })

    // 票 18（spec D7）：合并插件的真实贡献形态是**四目录 + 两个内置入口同时让位**
    // （设备配对 100 / 连接历史 101 / 终端会话 200 / Agent任务 210），故障半径已从
    // 「一个小插件」扩到「整个会话产品面」。摘除与恢复必须**整组同进同退**——
    // 半个产品面在位（如设备目录消失而会话目录留着）比全部消失更难解释，也更难修。
    it('合并插件四目录同进同退：两个内置入口一起让位、error 态整组摘除、恢复后整组回来', () => {
      const MERGED = 'com.bedcode.session'
      const panelIds = [
        ['session.pairing', BUILTIN_MENU_ORDERS.devices],
        ['session.history', BUILTIN_MENU_ORDERS.devices + 1],
        ['session.sidebar', BUILTIN_MENU_ORDERS.sessions],
        ['session.task-history', BUILTIN_MENU_ORDERS.sessions + 10],
      ].map(([viewId, order]) => registerPluginView(MERGED, viewId, 'sidebar', order))

      // 让位：两个内置入口都不在，四个贡献目录按 order 落在原位，末尾两项恒在最末
      expect(ids()).toEqual([...panelIds, 'plugins', 'settings'])
      expect(ids()).not.toContain('devices')
      expect(ids()).not.toContain('sessions')

      // error 态：四目录整组摘除，形态回落到「未装该插件」的默认菜单，不残留空目录
      usedPluginIds.add(MERGED)
      registry.setPluginState(MERGED, { state: 'Error', error: 'wasm trap in task domain' })
      expect(ids()).toEqual(['devices', 'sessions', 'plugins', 'settings'])

      // 恢复激活：整组回来，且让位判据与摘除共用同一处（不出现两个入口）
      registry.setPluginState(MERGED, { state: 'Activated' })
      expect(ids()).toEqual([...panelIds, 'plugins', 'settings'])
    })

    it('未登记运行态的插件（激活未完成）其贡献目录不出现', () => {
      usedPluginIds.add('p-activating')
      disposables.push(
        registry.registerView('p-activating', 'sidebar', {
          id: 'v',
          title: 'activating',
          order: BUILTIN_MENU_ORDERS.devices,
          component: {},
        }),
      )
      registry.setPluginState('p-activating', { state: 'Activating' })

      expect(ids()).toEqual(['devices', 'sessions', 'plugins', 'settings'])
    })

    it('贡献目录落在「终端会话」槽位时只顶替会话入口，设备入口不受影响', () => {
      registerPluginView('com.bedcode.session', 'sessions', 'sidebar', BUILTIN_MENU_ORDERS.sessions)

      expect(ids()).toEqual([
        'devices',
        'plugin-com.bedcode.session-sessions',
        'plugins',
        'settings',
      ])
    })

    it('非让位内置入口（插件管理 / 设置）恒不被顶替', () => {
      registerPluginView('p1', 'v1', 'sidebar', BUILTIN_MENU_ORDERS.plugins)
      registerPluginView('p2', 'v2', 'sidebar', BUILTIN_MENU_ORDERS.settings)

      const list = ids()
      expect(list).toContain('plugins')
      expect(list).toContain('settings')
      expect(list).toContain('plugin-p1-v1')
      expect(list).toContain('plugin-p2-v2')
    })

    it('toolbox 贡献不顶替内置侧边栏入口（只有 sidebar 类型参与让位）', () => {
      registerPluginView('p1', 'v1', 'toolbox', BUILTIN_MENU_ORDERS.devices)

      expect(ids()).toEqual(['devices', 'plugin-p1-v1', 'sessions', 'plugins', 'settings'])
    })
  })
})

// ==================== 让位判据本身（纯函数边界） ====================

describe('builtinSupersededBy', () => {
  const active: Record<string, PluginState> = { p1: { state: 'Activated' } }

  function view(order: number, viewType = 'sidebar'): ContributionViewRef {
    return { pluginId: 'p1', viewId: 'v1', viewType, order }
  }

  /** 被测内置入口：设备配对（order 100） */
  const devicesItem = { order: BUILTIN_MENU_ORDERS.devices }

  it('贡献目录占用同一 order 槽位即顶替', () => {
    expect(builtinSupersededBy(devicesItem, [view(100)], active)).toEqual(view(100))
  })

  it('插在两槽之间（101 / 150）只是排序插入，不顶替设备入口', () => {
    expect(builtinSupersededBy(devicesItem, [view(101)], active)).toBeNull()
    expect(builtinSupersededBy(devicesItem, [view(150)], active)).toBeNull()
  })

  it('落在下一域槽位（200）不顶替设备入口', () => {
    expect(
      builtinSupersededBy(devicesItem, [view(BUILTIN_MENU_ORDERS.sessions)], active),
    ).toBeNull()
  })

  it('非运行态插件的贡献不触发让位', () => {
    const errored: Record<string, PluginState> = { p1: { state: 'Error', error: 'trap' } }
    expect(builtinSupersededBy(devicesItem, [view(100)], errored)).toBeNull()
    expect(builtinSupersededBy(devicesItem, [view(100)], {})).toBeNull()
  })

  it('内置入口清单中只有设备配对与终端会话声明让位，插件管理与设置不让位', () => {
    expect(builtinMenuItems.filter((i) => i.supersedable).map((i) => i.id)).toEqual([
      'sessions',
      'devices',
    ])
    expect(builtinMenuItems.find((i) => i.id === 'plugins')?.supersedable).toBeFalsy()
    expect(builtinMenuItems.find((i) => i.id === 'settings')?.supersedable).toBeFalsy()
  })
})
