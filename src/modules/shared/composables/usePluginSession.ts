//! Plugin Session Composable
//!
//! Vue composable for managing plugin-based sessions (SCM, etc.)

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SessionInfo } from '@/modules/desktop/composables/model'

// Re-export from model
import type { PluginSessionInfo } from './model'
export type { PluginSessionInfo }

export function usePluginSession() {
  const sessions = ref<PluginSessionInfo[]>([])
  const loading = ref(false)

  async function loadPluginSessions() {
    loading.value = true
    try {
      const allSessions = await invoke<SessionInfo[]>('list_sessions')
      sessions.value = allSessions
        .filter(s => (s.session_type || s.sessionType) === 'plugin')
        .map(s => ({
          id: s.id,
          name: s.name,
          config_id: s.config_id || s.configId || '',
          status: s.status,
          session_type: 'plugin',
          created_at: s.created_at || s.createdAt || new Date().toISOString(),
        }))
    } catch (e) {
      console.error('Failed to load plugin sessions:', e)
    } finally {
      loading.value = false
    }
  }

  return {
    sessions,
    loading,
    loadPluginSessions,
  }
}
