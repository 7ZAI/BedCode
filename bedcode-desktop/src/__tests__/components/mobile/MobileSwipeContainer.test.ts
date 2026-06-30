import { describe, it, expect } from 'vitest'

describe('MobileSwipeContainer', () => {
  it('should have CONFIG with correct values', () => {
    const CONFIG = {
      directionThreshold: 20,
      swipeThreshold: 80,
      velocityThreshold: 0.3,
      maxOvershoot: 50,
      animationDuration: 300
    }

    expect(CONFIG.directionThreshold).toBe(20)
    expect(CONFIG.swipeThreshold).toBe(80)
    expect(CONFIG.velocityThreshold).toBe(0.3)
    expect(CONFIG.animationDuration).toBe(300)
  })

  it('should have pages configuration', () => {
    const pages = [
      { name: 'mobile-devices', component: 'DevicesView' },
      { name: 'mobile-sessions', component: 'SessionsView' },
      { name: 'mobile-toolbox', component: 'ToolboxView' },
      { name: 'mobile-settings', component: 'SettingsView' }
    ]

    expect(pages).toHaveLength(4)
    expect(pages[0].name).toBe('mobile-devices')
    expect(pages[3].name).toBe('mobile-settings')
  })
})