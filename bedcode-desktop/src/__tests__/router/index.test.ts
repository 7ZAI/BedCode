import { describe, it, expect, beforeEach, vi } from 'vitest'
import router from '@/router'

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
})
