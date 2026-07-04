import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useQuickActionStore } from '@/stores/quickAction'

describe('useQuickActionStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('should start with null pendingInput', () => {
    const store = useQuickActionStore()
    expect(store.pendingInput).toBeNull()
  })

  it('should set pending input', () => {
    const store = useQuickActionStore()
    store.setPendingInput('hello')
    expect(store.pendingInput).toBe('hello')
  })

  it('should consume and clear pending input', () => {
    const store = useQuickActionStore()
    store.setPendingInput('test command')
    expect(store.consumePendingInput()).toBe('test command')
    expect(store.pendingInput).toBeNull()
  })
})
