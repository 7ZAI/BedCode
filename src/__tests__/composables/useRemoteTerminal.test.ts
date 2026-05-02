import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'

describe('useRemoteTerminal', () => {
  let mockConnection: any

  beforeEach(() => {
    vi.clearAllMocks()
    mockConnection = {
      state: ref({ status: 'connected' }),
      isConnected: ref(true),
      lastMessage: ref(null),
      sendMessage: vi.fn().mockReturnValue(true),
      sendMessageWithResponse: vi.fn().mockResolvedValue({
        payload: { action: { type: 'session_list', sessions: [] } },
      }),
    }
  })

  it('should initialize with empty sessions', () => {
    const { sessions } = useRemoteTerminal(mockConnection)
    expect(sessions.value).toEqual([])
  })

  it('should have null currentSessionId initially', () => {
    const { currentSessionId } = useRemoteTerminal(mockConnection)
    expect(currentSessionId.value).toBeNull()
  })

  it('should have empty outputBuffer initially', () => {
    const { outputBuffer } = useRemoteTerminal(mockConnection)
    expect(outputBuffer.value).toEqual([])
  })

  it('should send input when connected and session is selected', () => {
    const { sendInput, currentSessionId } = useRemoteTerminal(mockConnection)
    currentSessionId.value = 'test-session'

    sendInput('hello')

    expect(mockConnection.sendMessage).toHaveBeenCalledWith(
      'input',
      { data: 'hello', special_key: null },
      'test-session'
    )
  })

  it('should send special key when connected and session is selected', () => {
    const { sendSpecialKey, currentSessionId } = useRemoteTerminal(mockConnection)
    currentSessionId.value = 'test-session'

    sendSpecialKey('ctrl_c')

    expect(mockConnection.sendMessage).toHaveBeenCalledWith(
      'input',
      { data: '', special_key: 'ctrl_c' },
      'test-session'
    )
  })
})
