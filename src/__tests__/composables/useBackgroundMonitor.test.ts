import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { useBackgroundMonitor } from '@/composables/useBackgroundMonitor'

// Mock Tauri APIs
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}))

// Mock usePlatform to return mobile platform
vi.mock('@/composables/usePlatform', () => ({
  usePlatform: () => ({
    platformInfo: {
      value: {
        isMobile: true,
        isDesktop: false,
      },
    },
  }),
}))

describe('useBackgroundMonitor', () => {
  it('should export isInBackground and wasInBackground refs', () => {
    const { isInBackground, wasInBackground } = useBackgroundMonitor()
    expect(isInBackground).toBeDefined()
    expect(wasInBackground).toBeDefined()
    expect(isInBackground.value).toBe(false)
    expect(wasInBackground.value).toBe(false)
  })
})