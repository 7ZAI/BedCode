import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { useWebSocket } from '@/composables/useWebSocket'

// Mock WebSocket
class MockWebSocket {
  static CONNECTING = 0
  static OPEN = 1
  static CLOSING = 2
  static CLOSED = 3

  readyState = MockWebSocket.OPEN
  onopen: ((event: Event) => void) | null = null
  onclose: ((event: CloseEvent) => void) | null = null
  onmessage: ((event: MessageEvent) => void) | null = null
  onerror: ((event: Event) => void) | null = null

  constructor(public url: string) {
    setTimeout(() => {
      this.onopen?.(new Event('open'))
    }, 0)
  }

  send(data: string) {}
  close(code?: number, reason?: string) {
    this.readyState = MockWebSocket.CLOSED
    this.onclose?.(new CloseEvent('close', { code: code || 1000, reason: reason || '' }))
  }
}

// Replace global WebSocket
vi.stubGlobal('WebSocket', MockWebSocket)

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

describe('useWebSocket', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('should initialize with default state', () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { isConnected, connectionError, reconnectAttempts } = result

    expect(isConnected.value).toBe(false)
    expect(connectionError.value).toBeNull()
    expect(reconnectAttempts.value).toBe(0)

    wrapper.unmount()
  })

  it('should connect to WebSocket', async () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { connect, isConnected } = result

    connect('192.168.1.100', 8765)

    // Wait for onopen callback
    await vi.runAllTimersAsync()

    expect(isConnected.value).toBe(true)

    wrapper.unmount()
  })

  it('should send message when connected', async () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { connect, sendMessage, isConnected } = result

    connect('192.168.1.100', 8765)
    await vi.runAllTimersAsync()

    const sendResult = sendMessage('input', { data: 'test' }, 'session-1')

    expect(sendResult).toBe(true)

    wrapper.unmount()
  })

  it('should not send message when disconnected', () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { sendMessage, isConnected } = result

    expect(isConnected.value).toBe(false)

    const sendResult = sendMessage('input', { data: 'test' }, 'session-1')

    expect(sendResult).toBe(false)

    wrapper.unmount()
  })

  it('should send input with special key', async () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { connect, sendInput, isConnected } = result

    connect('192.168.1.100', 8765)
    await vi.runAllTimersAsync()

    const sendResult = sendInput('Hello', 'session-1', 'ctrl_c')

    expect(sendResult).toBe(true)

    wrapper.unmount()
  })

  it('should send special key', async () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { connect, sendSpecialKey } = result

    connect('192.168.1.100', 8765)
    await vi.runAllTimersAsync()

    const sendResult = sendSpecialKey('escape', 'session-1')

    expect(sendResult).toBe(true)

    wrapper.unmount()
  })

  it('should disconnect properly', async () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { connect, disconnect, isConnected } = result

    connect('192.168.1.100', 8765)
    await vi.runAllTimersAsync()

    expect(isConnected.value).toBe(true)

    disconnect()

    expect(isConnected.value).toBe(false)

    wrapper.unmount()
  })

  it('should handle resize message', async () => {
    const { result, wrapper } = withComposable(() => useWebSocket())
    const { connect, resize } = result

    connect('192.168.1.100', 8765)
    await vi.runAllTimersAsync()

    const resizeResult = resize(120, 40, 'session-1')

    expect(resizeResult).toBe(true)

    wrapper.unmount()
  })
})
