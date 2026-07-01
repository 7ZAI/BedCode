import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, nextTick } from 'vue'
import { useToast } from '@/composables/useToast'

// Helper to use composable in component context
function withComposable<T>(composable: () => T) {
  let result: T

  const TestComponent = defineComponent({
    setup() {
      result = composable()
      return {}
    },
    template: '<div></div>',
  })

  const wrapper = mount(TestComponent)

  return {
    get result() {
      return result!
    },
    wrapper,
  }
}

describe('useToast', () => {
  let toastInstance: ReturnType<typeof useToast>

  beforeEach(() => {
    vi.useFakeTimers()
    // Get fresh instance and clear any existing toasts
    const { result, wrapper } = withComposable(() => useToast())
    toastInstance = result
    result.dismissAll()
    wrapper.unmount()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  describe('initial state', () => {
    it('should have empty toasts after reset', () => {
      const { result, wrapper } = withComposable(() => useToast())

      expect(result.toasts.value).toEqual([])

      wrapper.unmount()
    })
  })

  describe('show', () => {
    it('should add a toast to the array', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      const id = result.show({ message: 'Test message' })

      expect(result.toasts.value).toHaveLength(1)
      expect(result.toasts.value[0].id).toBe(id)
      expect(result.toasts.value[0].options.message).toBe('Test message')

      wrapper.unmount()
    })

    it('should return unique incrementing IDs', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      const id1 = result.show({ message: 'First' })
      const id2 = result.show({ message: 'Second' })
      const id3 = result.show({ message: 'Third' })

      expect(id1).toBeLessThan(id2)
      expect(id2).toBeLessThan(id3)

      wrapper.unmount()
    })

    it('should accept all toast options', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.show({
        message: 'Full options toast',
        type: 'error',
        duration: 5000,
        position: 'bottom',
      })

      const toast = result.toasts.value[0]
      expect(toast.options.message).toBe('Full options toast')
      expect(toast.options.type).toBe('error')
      expect(toast.options.duration).toBe(5000)
      expect(toast.options.position).toBe('bottom')

      wrapper.unmount()
    })
  })

  describe('convenience methods', () => {
    it('success should create success toast', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.success('Operation succeeded')

      const toast = result.toasts.value[0]
      expect(toast.options.message).toBe('Operation succeeded')
      expect(toast.options.type).toBe('success')
      expect(toast.options.duration).toBe(3000)

      wrapper.unmount()
    })

    it('success should accept custom duration', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.success('Custom duration', 10000)

      expect(result.toasts.value[0].options.duration).toBe(10000)

      wrapper.unmount()
    })

    it('error should create error toast with longer default duration', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.error('Something went wrong')

      const toast = result.toasts.value[0]
      expect(toast.options.message).toBe('Something went wrong')
      expect(toast.options.type).toBe('error')
      expect(toast.options.duration).toBe(5000)

      wrapper.unmount()
    })

    it('warning should create warning toast', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.warning('Be careful')

      const toast = result.toasts.value[0]
      expect(toast.options.message).toBe('Be careful')
      expect(toast.options.type).toBe('warning')
      expect(toast.options.duration).toBe(4000)

      wrapper.unmount()
    })

    it('info should create info toast', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.info('For your information')

      const toast = result.toasts.value[0]
      expect(toast.options.message).toBe('For your information')
      expect(toast.options.type).toBe('info')
      expect(toast.options.duration).toBe(3000)

      wrapper.unmount()
    })
  })

  describe('dismiss', () => {
    it('should remove toast by id', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      const id1 = result.show({ message: 'First' })
      const id2 = result.show({ message: 'Second' })

      expect(result.toasts.value).toHaveLength(2)

      result.dismiss(id1)

      expect(result.toasts.value).toHaveLength(1)
      expect(result.toasts.value[0].id).toBe(id2)

      wrapper.unmount()
    })

    it('should do nothing if id not found', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.show({ message: 'Only toast' })

      expect(result.toasts.value).toHaveLength(1)

      result.dismiss(99999) // Non-existent ID

      expect(result.toasts.value).toHaveLength(1)

      wrapper.unmount()
    })

    it('should dismiss multiple toasts independently', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      const id1 = result.show({ message: 'First' })
      const id2 = result.show({ message: 'Second' })
      const id3 = result.show({ message: 'Third' })

      expect(result.toasts.value).toHaveLength(3)

      result.dismiss(id2)

      expect(result.toasts.value).toHaveLength(2)
      expect(result.toasts.value.map((t) => t.id)).toEqual([id1, id3])

      wrapper.unmount()
    })
  })

  describe('dismissAll', () => {
    it('should remove all toasts', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.show({ message: 'First' })
      result.show({ message: 'Second' })
      result.show({ message: 'Third' })

      expect(result.toasts.value).toHaveLength(3)

      result.dismissAll()

      expect(result.toasts.value).toEqual([])

      wrapper.unmount()
    })

    it('should work on empty array', () => {
      const { result, wrapper } = withComposable(() => useToast())

      result.dismissAll() // Clear first
      result.dismissAll() // Should not throw

      expect(result.toasts.value).toEqual([])

      wrapper.unmount()
    })
  })

  describe('multiple instances', () => {
    it('should share toast state between instances', () => {
      const { result: instance1, wrapper: w1 } = withComposable(() => useToast())

      instance1.dismissAll() // Clear first
      instance1.show({ message: 'From instance 1' })

      // Both instances should see the same toast
      expect(instance1.toasts.value).toHaveLength(1)
      expect(instance1.toasts.value[0].options.message).toBe('From instance 1')

      w1.unmount()
    })
  })
})
