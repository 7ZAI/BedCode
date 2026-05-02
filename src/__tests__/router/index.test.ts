import { describe, it, expect, beforeEach, vi } from 'vitest'
import router from '@/router'

describe('Router Configuration', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('route definitions', () => {
    it('should have root redirect to sessions', () => {
      const route = router.getRoutes().find((r) => r.path === '/')

      expect(route).toBeDefined()
      expect(route?.redirect).toBe('/sessions')
    })

    it('should have sessions route', () => {
      const route = router.getRoutes().find((r) => r.path === '/sessions')

      expect(route).toBeDefined()
      expect(route?.name).toBe('sessions')
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
  })

  describe('mobile routes', () => {
    it('should have mobile devices route', () => {
      const route = router.getRoutes().find((r) => r.path === '/mobile/devices')

      expect(route).toBeDefined()
      expect(route?.name).toBe('mobile-devices')
    })

    it('should have mobile terminal route with id param', () => {
      const route = router.getRoutes().find((r) => r.path === '/mobile/terminal/:id')

      expect(route).toBeDefined()
      expect(route?.name).toBe('mobile-terminal')
    })

    it('should have mobile quick-actions route', () => {
      const route = router.getRoutes().find((r) => r.path === '/mobile/quick-actions')

      expect(route).toBeDefined()
      expect(route?.name).toBe('mobile-quick-actions')
    })

    it('should have mobile history route', () => {
      const route = router.getRoutes().find((r) => r.path === '/mobile/history')

      expect(route).toBeDefined()
      expect(route?.name).toBe('mobile-history')
    })

    it('should have mobile settings route', () => {
      const route = router.getRoutes().find((r) => r.path === '/mobile/settings')

      expect(route).toBeDefined()
      expect(route?.name).toBe('mobile-settings')
    })
  })

  describe('route structure', () => {
    it('should have correct route names', () => {
      const expectedNames = [
        'sessions',
        'devices',
        'settings',
        'mobile-devices',
        'mobile-terminal',
        'mobile-quick-actions',
        'mobile-history',
        'mobile-settings',
      ]

      const routes = router.getRoutes()
      expectedNames.forEach((name) => {
        const route = routes.find((r) => r.name === name)
        expect(route).toBeDefined()
      })
    })

    it('should have correct paths for desktop routes', () => {
      const desktopPaths = ['/sessions', '/devices', '/settings']

      desktopPaths.forEach((path) => {
        const route = router.getRoutes().find((r) => r.path === path)
        expect(route).toBeDefined()
      })
    })

    it('should have correct paths for mobile routes', () => {
      const mobilePaths = [
        '/mobile/devices',
        '/mobile/terminal/:id',
        '/mobile/quick-actions',
        '/mobile/history',
        '/mobile/settings',
      ]

      mobilePaths.forEach((path) => {
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
      // Check that router has history property
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

      // Count includes redirect routes and component routes
      // Desktop: /, /sessions, /devices, /settings
      // Mobile: /mobile/devices, /mobile/terminal/:id, /mobile/quick-actions, /mobile/history, /mobile/settings
      expect(routes.length).toBeGreaterThanOrEqual(9)
    })
  })

  describe('route meta and properties', () => {
    it('should have dynamic param for mobile terminal', () => {
      const route = router.getRoutes().find((r) => r.name === 'mobile-terminal')

      expect(route?.path).toContain(':id')
    })
  })
})
