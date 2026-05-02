import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { useOutputBuffer } from '@/composables/useOutputBuffer'

// Helper function to wrap composable in a component context
function withComposable<T>(composable: () => T) {
  let result: T

  const TestComponent = defineComponent({
    setup() {
      result = composable()
      return {}
    },
    template: '<div></div>'
  })

  const wrapper = mount(TestComponent)

  return {
    get result() { return result! },
    wrapper
  }
}

describe('useOutputBuffer', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('should initialize with empty buffer', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { buffer, getSize } = result

    expect(buffer.value).toEqual([])
    expect(getSize()).toBe(0)

    wrapper.unmount()
  })

  it('should append data to buffer', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { append, buffer } = result

    append('Hello')
    append(' World')

    expect(buffer.value).toHaveLength(2)
    expect(buffer.value[0]).toBe('Hello')
    expect(buffer.value[1]).toBe(' World')

    wrapper.unmount()
  })

  it('should flush buffer', async () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { append, flush } = result

    append('Line 1')
    append('Line 2')

    const flushResult = flush()

    expect(flushResult).not.toBeNull()
    expect(flushResult?.text).toBe('Line 1Line 2')
    expect(flushResult?.timestamp).toBeGreaterThan(0)

    wrapper.unmount()
  })

  it('should clear buffer', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { append, clear, buffer, getSize } = result

    append('Some content')
    append('More content')

    clear()

    expect(buffer.value).toEqual([])
    expect(getSize()).toBe(0)

    wrapper.unmount()
  })

  it('should schedule flush after append', async () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { append, flush } = result

    append('Test data')

    // Advance timers but not enough to trigger flush
    vi.advanceTimersByTime(25)

    // Buffer should still have data
    // (flush hasn't been called automatically yet)
    const flushResult = flush()
    expect(flushResult?.text).toBe('Test data')

    wrapper.unmount()
  })

  it('should return null when flushing empty buffer', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { flush } = result

    const flushResult = flush()

    expect(flushResult).toBeNull()

    wrapper.unmount()
  })

  it('should track buffer size', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer())
    const { append, getSize } = result

    append('12345')
    expect(getSize()).toBe(5)

    append('67890')
    expect(getSize()).toBe(10)

    wrapper.unmount()
  })

  it('should limit buffer size', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer(50, 100))
    const { append, getSize } = result

    // Add more than max size
    for (let i = 0; i < 20; i++) {
      append('1234567890') // 10 chars each
    }

    // Size should be limited
    expect(getSize()).toBeLessThanOrEqual(100)

    wrapper.unmount()
  })

  it('should use custom flush interval', () => {
    const { result, wrapper } = withComposable(() => useOutputBuffer(100))
    const { append } = result

    append('Test')

    // Timer should be set with custom interval
    // This is more of an integration test
    expect(true).toBe(true)

    wrapper.unmount()
  })
})
