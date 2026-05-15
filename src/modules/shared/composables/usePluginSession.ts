//! Plugin Session Composable
//!
//! Vue composable for managing plugin-based sessions (SCM, etc.)

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SessionInfo } from './useTauri'

export interface PluginSessionInfo {
  id: string
  name: string
  status: string
  sessionType: 'pty' | 'plugin'
  projectPath?: string
}

export function usePluginSession() {
  const sessions = ref<PluginSessionInfo[]>([])
  const loading = ref(false)

  async function loadPluginSessions() {
    loading.value = true
    try {
      const allSessions = await invoke<SessionInfo[]>('list_sessions')
      sessions.value = allSessions
        .filter(s => s.sessionType === 'plugin')
        .map(s => ({
          id: s.id,
          name: s.name,
          status: s.status,
          sessionType: 'plugin' as const,
          projectPath: s.configId || '',
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
