import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { useToast } from '@/composables/useToast'

// Mock vue-sonner：断言各类型分发到对应 toast 方法与参数
vi.mock('vue-sonner', () => ({
  toast: {
    success: vi.fn(() => 'mock-id-1'),
    error: vi.fn(() => 'mock-id-2'),
    warning: vi.fn(() => 'mock-id-3'),
    info: vi.fn(() => 'mock-id-4'),
    dismiss: vi.fn(),
  },
}))

import { toast } from 'vue-sonner'
const mockedToast = vi.mocked(toast)

describe('useToast', () => {
  let toastInstance: ReturnType<typeof useToast>

  beforeEach(() => {
    vi.clearAllMocks()
    toastInstance = useToast()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  describe('show', () => {
    it('should dispatch info by default when type is not given', () => {
      toastInstance.show({ message: 'Default info' })

      expect(mockedToast.info).toHaveBeenCalledWith('Default info', {
        duration: undefined,
        position: 'top-center',
      })
    })

    it('should dispatch to the matching toast method with options', () => {
      toastInstance.show({
        message: 'Full options toast',
        type: 'error',
        duration: 5000,
        position: 'bottom',
      })

      expect(mockedToast.error).toHaveBeenCalledWith('Full options toast', {
        duration: 5000,
        position: 'bottom-center',
      })
    })

    it('should return the id from sonner', () => {
      const id = toastInstance.show({ message: 'Test' })

      expect(id).toBe('mock-id-4')
    })
  })

  describe('convenience methods', () => {
    it('success should call toast.success with default duration', () => {
      toastInstance.success('Operation succeeded')

      expect(mockedToast.success).toHaveBeenCalledWith('Operation succeeded', {
        duration: 3000,
        position: 'top-center',
      })
    })

    it('success should accept custom duration', () => {
      toastInstance.success('Custom duration', 10000)

      expect(mockedToast.success).toHaveBeenCalledWith('Custom duration', {
        duration: 10000,
        position: 'top-center',
      })
    })

    it('error should call toast.error with longer default duration', () => {
      toastInstance.error('Something went wrong')

      expect(mockedToast.error).toHaveBeenCalledWith('Something went wrong', {
        duration: 5000,
        position: 'top-center',
      })
    })

    it('warning should call toast.warning', () => {
      toastInstance.warning('Be careful')

      expect(mockedToast.warning).toHaveBeenCalledWith('Be careful', {
        duration: 4000,
        position: 'top-center',
      })
    })

    it('info should call toast.info', () => {
      toastInstance.info('For your information')

      expect(mockedToast.info).toHaveBeenCalledWith('For your information', {
        duration: 3000,
        position: 'top-center',
      })
    })
  })

  describe('dismiss', () => {
    it('should forward id to sonner dismiss', () => {
      toastInstance.dismiss('mock-id-1')

      expect(mockedToast.dismiss).toHaveBeenCalledWith('mock-id-1')
    })

    it('should forward numeric id to sonner dismiss', () => {
      toastInstance.dismiss(42)

      expect(mockedToast.dismiss).toHaveBeenCalledWith(42)
    })
  })

  describe('dismissAll', () => {
    it('should call sonner dismiss without args', () => {
      toastInstance.dismissAll()

      expect(mockedToast.dismiss).toHaveBeenCalledWith()
    })
  })
})
