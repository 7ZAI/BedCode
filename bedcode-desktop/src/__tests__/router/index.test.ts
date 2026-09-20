import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import router from '@/router'
import { getPluginRegistry } from '@/plugin/registry'
import { BUILTIN_MENU_ORDERS } from '@/composables/useSidebarMenu'

// 懒激活守卫会调用后端命令，测试环境无 Tauri：桩掉加载器，只验路由让位/兜底判定本身
vi.mock('@/plugin/loader', () => ({
  pluginLoader: {
    getActivePlugin: vi.fn(() => ({ id: 'com.bedcode.session' })),
    activate: vi.fn(async () => {}),
  },
}))

describe('Router Configuration', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('route definitions', () => {
    it('should have root route redirecting to sessions', () => {
      const route = router.getRoutes().find((r) => r.path === '/')

      expect(route).toBeDefined()
      expect(route?.redirect).toBe('/sessions')
    })

    it('should have sessions route', () => {
      const route = router.getRoutes().find((r) => r.path === '/sessions')

      expect(route).toBeDefined()
      expect(route?.name).toBe('session')
    })

    it('should have server route', () => {
      const route = router.getRoutes().find((r) => r.path === '/server')

      expect(route).toBeDefined()
      expect(route?.name).toBe('server')
    })

    it('should have devices route', () => {
      const route = router.getRoutes().find((r) => r.path === '/devices')

      expect(route).toBeDefined()
      expect(route?.name).toBe('devices')
    })

    it('should have settings route', () => {
      const route = router.getRoutes().find((r) => r.path === '/settings')

      expect(route).toBeDefined()
      expect(route?.name).toBe('settings')
    })

    it('should have plugins route', () => {
      const route = router.getRoutes().find((r) => r.path === '/plugins')

      expect(route).toBeDefined()
      expect(route?.name).toBe('plugins')
    })
  })

  describe('detail routes', () => {
    it('should have device history route with id param', () => {
      const route = router.getRoutes().find((r) => r.path === '/devices/:id/history')

      expect(route).toBeDefined()
      expect(route?.name).toBe('device-history')
      expect(route?.path).toContain(':id')
    })

    it('should have plugin config route with id param', () => {
      const route = router.getRoutes().find((r) => r.path === '/plugins/:id/config')

      expect(route).toBeDefined()
      expect(route?.name).toBe('plugin-config')
      expect(route?.path).toContain(':id')
    })

    it('should have plugin sidebar view route', () => {
      const route = router.getRoutes().find((r) => r.name === 'plugin-sidebar-view')

      expect(route).toBeDefined()
      expect(route?.path).toBe('/plugin/sidebar/:pluginId/:viewId')
    })

    it('should have plugin toolbox view route', () => {
      const route = router.getRoutes().find((r) => r.name === 'plugin-toolbox-view')

      expect(route).toBeDefined()
      expect(route?.path).toBe('/plugin/toolbox/:pluginId/:viewId')
    })

    it('should have terminal window route with id param', () => {
      const route = router.getRoutes().find((r) => r.path === '/terminal-window/:id')

      expect(route).toBeDefined()
      expect(route?.name).toBe('terminal-window')
      expect(route?.path).toContain(':id')
    })
  })

  describe('route structure', () => {
    it('should have correct route names', () => {
      const expectedNames = [
        'session',
        'server',
        'devices',
        'device-history',
        'settings',
        'plugins',
        'plugin-config',
        'plugin-sidebar-view',
        'plugin-toolbox-view',
        'terminal-window',
      ]

      const routes = router.getRoutes()
      expectedNames.forEach((name) => {
        const route = routes.find((r) => r.name === name)
        expect(route).toBeDefined()
      })
    })

    it('should have correct paths for desktop routes', () => {
      const desktopPaths = ['/sessions', '/server', '/devices', '/settings', '/plugins']

      desktopPaths.forEach((path) => {
        const route = router.getRoutes().find((r) => r.path === path)
        expect(route).toBeDefined()
      })
    })
  })

  describe('router instance', () => {
    it('should be a valid router instance', () => {
      expect(router).toBeDefined()
      expect(router.currentRoute).toBeDefined()
      expect(router.push).toBeDefined()
      expect(router.replace).toBeDefined()
    })

    it('should use HTML5 history mode', () => {
      expect(router.options.history).toBeDefined()
    })

    it('should have routes defined', () => {
      const routes = router.getRoutes()
      expect(routes.length).toBeGreaterThan(0)
    })
  })

  describe('route total count', () => {
    it('should have all expected routes', () => {
      const routes = router.getRoutes()

      // 包含 redirect 路由和全部业务路由
      expect(routes.length).toBeGreaterThanOrEqual(11)
    })
  })

  describe('route meta and properties', () => {
    it('should have dynamic param for terminal window', () => {
      const route = router.getRoutes().find((r) => r.name === 'terminal-window')

      expect(route?.path).toContain(':id')
    })

    it('should have dynamic params for plugin view routes', () => {
      const sidebar = router.getRoutes().find((r) => r.name === 'plugin-sidebar-view')
      const toolbox = router.getRoutes().find((r) => r.name === 'plugin-toolbox-view')

      expect(sidebar?.path).toContain(':pluginId')
      expect(sidebar?.path).toContain(':viewId')
      expect(toolbox?.path).toContain(':pluginId')
      expect(toolbox?.path).toContain(':viewId')
    })
  })

  // ==================== 内置入口让位后的深链兜底（票 02） ====================

  describe('内置入口深链兜底', () => {
    const registry = getPluginRegistry()
    const PLUGIN_ID = 'com.bedcode.session'

    /** 模拟插件贡献一个接管「设备配对」槽位的侧边栏目录 */
    function contributeDevicesView(): void {
      registry.setPluginState(PLUGIN_ID, { state: 'Activated' })
      registry.registerView(PLUGIN_ID, 'sidebar', {
        id: 'pairing',
        title: '设备与配对',
        order: BUILTIN_MENU_ORDERS.devices,
        component: {},
      })
    }

    afterEach(() => {
      registry.clearPlugin(PLUGIN_ID)
    })

    it('内置入口已被插件接管时，深链重定向到该贡献目录而非 404', async () => {
      contributeDevicesView()

      await router.push('/devices')

      expect(router.currentRoute.value.name).toBe('plugin-sidebar-view')
      expect(router.currentRoute.value.params).toMatchObject({
        pluginId: PLUGIN_ID,
        viewId: 'pairing',
      })
    })

    it('插件进入 error 态后同一深链回落渲染宿主兜底页', async () => {
      contributeDevicesView()
      registry.setPluginState(PLUGIN_ID, { state: 'Error', error: 'wasm trap' })

      await router.push('/devices')

      expect(router.currentRoute.value.name).toBe('devices')
    })

    it('无插件接管时内置路由直达，不被重定向', async () => {
      await router.push('/sessions')

      expect(router.currentRoute.value.name).toBe('session')
    })

    it('非让位内置路由（插件管理）即使同槽有贡献也直达宿主页', async () => {
      registry.setPluginState(PLUGIN_ID, { state: 'Activated' })
      registry.registerView(PLUGIN_ID, 'sidebar', {
        id: 'v',
        title: 'x',
        order: BUILTIN_MENU_ORDERS.plugins,
        component: {},
      })

      await router.push('/plugins')

      expect(router.currentRoute.value.name).toBe('plugins')
    })
  })
})
