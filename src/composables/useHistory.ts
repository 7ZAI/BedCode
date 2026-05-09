import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface HistoryRecord {
  id: string
  type: 'input' | 'output'
  content: string
  sessionName: string
  timestamp: number
}

interface BackendMessage {
  id: string
  session_id: string
  message_type: string
  content: string
  timestamp: string
}

export function useHistory() {
  const messages = ref<HistoryRecord[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  function mapMessage(m: BackendMessage): HistoryRecord {
    return {
      id: m.id,
      type: m.message_type === 'input' ? 'input' : 'output',
      content: m.content,
      sessionName: m.session_id.slice(0, 8),
      timestamp: new Date(m.timestamp).getTime(),
    }
  }

  async function loadMessages(): Promise<void> {
    loading.value = true
    error.value = null
    try {
      const raw = await invoke<BackendMessage[]>('get_terminal_history', { limit: 200 })
      messages.value = raw.map(mapMessage)
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function searchMessages(query: string): Promise<void> {
    loading.value = true
    error.value = null
    try {
      const raw = await invoke<BackendMessage[]>('search_terminal_history', {
        query,
        limit: 100,
      })
      messages.value = raw.map(mapMessage)
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function clearSessionMessages(sessionId?: string): Promise<void> {
    try {
      if (sessionId) {
        messages.value = messages.value.filter(m => m.sessionName !== sessionId)
      } else {
        messages.value = []
      }
    } catch (e) {
      error.value = String(e)
    }
  }

  return {
    messages,
    loading,
    error,
    loadMessages,
    searchMessages,
    clearSessionMessages,
  }
}
