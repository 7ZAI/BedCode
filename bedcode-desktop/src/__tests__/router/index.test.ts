import { describe, it, expect, beforeEach, vi } from 'vitest'
import router from '@/router'

// 懒激活守卫会调用后端命令，测试环境无 Tauri：桩掉加载器，只验路由定义本身
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
    it('should have root route redirecting to plugins', () => {
      const route = router.getRoutes().find((r) => r.path === '/')

      expect(route).toBeDefined()
      // 服务器页已无侧边栏入口（服务器常驻不可开关），落地页取插件管理
      expect(route?.redirect).toBe('/plugins')
    })

    it('should have server route', () => {
      const route = router.getRoutes().find((r) => r.path === '/server')

      expect(route).toBeDefined()
      expect(route?.name).toBe('server')
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
        'server',
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
      const desktopPaths = ['/server', '/settings', '/plugins']

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

      // 包含 redirect 路由和全部业务路由（会话 / 设备配对兜底路由已随票 13/14 删除）
      expect(routes.length).toBeGreaterThanOrEqual(9)
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
})
