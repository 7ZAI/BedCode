import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { useSidebarMenu, registerSidebarItem } from '@/composables/useSidebarMenu'
import { getPluginRegistry } from '@/plugin/registry'

/**
 * useSidebarMenu 测试 — 统一菜单合并与顺序扩展点
 *
 * 覆盖：内置项与插件视图合并、插件 order 插入内置项之间、registerSidebarItem 扩展点
 */
describe('useSidebarMenu', () => {
  const registry = getPluginRegistry()
  const disposables: { dispose: () => void }[] = []

  /** 注册一个插件视图（sidebar 或 toolbox），返回其菜单项 id */
  function registerPluginView(pluginId: string, viewId: string, viewType: string, order?: number): string {
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
    expect(ids).toEqual(['devices', 'sessions', 'plugin-p1-v1', 'plugin-p2-v2', 'plugins', 'settings'])
  })

  it('插件可通过 order 插入到任意内置菜单项之间', () => {
    // order 150：位于"设备配对"(100) 与"终端会话"(200) 之间
    registerPluginView('p1', 'v1', 'sidebar', 150)
    // order 350：位于"终端会话"(200) 与"插件"(400) 之间（server 槽位 300 已废弃）
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
    const custom = registerSidebarItem({ id: 'custom', path: '/custom', labelKey: 'Custom', order: 250 })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    expect(menuItems.value.map((m) => m.id)).toEqual(['devices', 'sessions', 'custom', 'plugins', 'settings'])

    // dispose 后菜单项移除
    custom.dispose()
    expect(menuItems.value.map((m) => m.id)).toEqual(['devices', 'sessions', 'plugins', 'settings'])
  })

  it('自定义项支持 i18n key 与纯文本标题标记', () => {
    const custom = registerSidebarItem({ id: 'i18n-item', path: '/x', labelKey: 'desktop.sidebar.session', isI18nKey: true, order: 1 })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    const item = menuItems.value.find((m) => m.id === 'i18n-item')!
    expect(item.isI18nKey).toBe(true)
    expect(item.labelKey).toBe('desktop.sidebar.session')
  })

  it('同 order 时保持 内置 → 自定义 → 插件 的稳定顺序', () => {
    registerPluginView('p1', 'v1', 'sidebar', 100)
    const custom = registerSidebarItem({ id: 'custom', path: '/custom', labelKey: 'Custom', order: 100 })
    disposables.push(custom)

    const { menuItems } = useSidebarMenu()
    const ids = menuItems.value.map((m) => m.id)
    // 与"设备配对"同 order=100：内置在前，自定义次之，插件最后
    expect(ids.indexOf('devices')).toBeLessThan(ids.indexOf('custom'))
    expect(ids.indexOf('custom')).toBeLessThan(ids.indexOf('plugin-p1-v1'))
  })
})
