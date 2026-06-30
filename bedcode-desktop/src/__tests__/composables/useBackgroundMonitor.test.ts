import { describe, it, expect, vi } from 'vitest'
import { ref } from 'vue'
import { useBackgroundMonitor } from '@/modules/shared/composables/useBackgroundMonitor'

// Mock useAndroidFeatures
vi.mock('@/modules/shared/composables/useAndroidFeatures', () => ({
  useAndroidFeatures: vi.fn(() => ({
    isInBackground: ref(false),
  })),
}))

describe('useBackgroundMonitor', () => {
  it('should return isInBackground from useAndroidFeatures', () => {
    const { isInBackground } = useBackgroundMonitor()
    expect(isInBackground).toBeDefined()
    expect(isInBackground.value).toBe(false)
  })

  it('should return wasInBackground ref initialized to false', () => {
    const { wasInBackground } = useBackgroundMonitor()
    expect(wasInBackground).toBeDefined()
    expect(wasInBackground.value).toBe(false)
  })

  it('should return clearWasInBackground function', () => {
    const { clearWasInBackground } = useBackgroundMonitor()
    expect(clearWasInBackground).toBeDefined()
    expect(typeof clearWasInBackground).toBe('function')
  })

  it('should allow clearing wasInBackground', () => {
    const { wasInBackground, clearWasInBackground } = useBackgroundMonitor()

    // Simulate wasInBackground being set
    ;(wasInBackground as any).value = true
    expect(wasInBackground.value).toBe(true)

    // Clear it
    clearWasInBackground()
    expect(wasInBackground.value).toBe(false)
  })
})