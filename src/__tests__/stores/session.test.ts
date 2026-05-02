import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSessionStore } from '@/stores/session'

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

// Mock Tauri event
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

describe('Session Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('should initialize with empty state', () => {
    const store = useSessionStore()

    expect(store.sessions).toEqual([])
    expect(store.activeSession).toBeNull()
    expect(store.configs).toEqual([])
  })

  it('should set active session', () => {
    const store = useSessionStore()

    const session = {
      id: 'test-id',
      config_id: 'config-1',
      status: 'running',
      started_at: Date.now(),
    }

    store.activeSession = session

    expect(store.activeSession).toEqual(session)
  })

  it('should add session to list', () => {
    const store = useSessionStore()

    const session = {
      id: 'test-id',
      config_id: 'config-1',
      status: 'running',
      started_at: Date.now(),
    }

    store.sessions.push(session)

    expect(store.sessions).toHaveLength(1)
    expect(store.sessions[0]).toEqual(session)
  })

  it('should remove session from list', () => {
    const store = useSessionStore()

    store.sessions = [
      { id: 'session-1', config_id: 'config-1', status: 'running', started_at: Date.now() },
      { id: 'session-2', config_id: 'config-2', status: 'running', started_at: Date.now() },
    ]

    store.sessions = store.sessions.filter(s => s.id !== 'session-1')

    expect(store.sessions).toHaveLength(1)
    expect(store.sessions[0].id).toBe('session-2')
  })

  it('should clear active session', () => {
    const store = useSessionStore()

    store.activeSession = {
      id: 'test-id',
      config_id: 'config-1',
      status: 'running',
      started_at: Date.now(),
    }

    store.activeSession = null

    expect(store.activeSession).toBeNull()
  })
})
