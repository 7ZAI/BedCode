import { defineStore } from 'pinia'
import { ref } from 'vue'
import {
  listSessions,
  startSession,
  killSession,
  deleteSession,
  restartSession,
  writeToSession,
  sendSpecialKey,
  resizeSession,
  listSessionConfigs,
  createSessionConfig,
  deleteSessionConfig,
  updateSessionConfig,
  type SessionConfig,
  type SessionInfo
} from '@/modules/desktop/composables/useDesktopCommands'

export { type SessionConfig, type SessionInfo }

export const useSessionStore = defineStore('session', () => {
  const sessions = ref<SessionInfo[]>([])
  const configs = ref<SessionConfig[]>([])
  const activeSession = ref<SessionInfo | null>(null)

  async function loadConfigs() {
    configs.value = await listSessionConfigs()
  }

  async function loadSessions() {
    sessions.value = await listSessions()
    console.log('loadSessions completed, sessions:', sessions.value.map(s => ({ id: s.id, status: s.status })))
  }

  async function createSession(configId: string) {
    const sessionId = await startSession(configId)
    sessions.value = await listSessions()
    console.log('createSession completed, sessionId:', sessionId, 'sessions:', sessions.value.map(s => ({ id: s.id, status: s.status })))

    // Find the new session and set as active
    const session = sessions.value.find(s => s.id === sessionId)
    if (session) {
      activeSession.value = session
    } else {
      console.warn('New session not found in sessions array!')
    }

    return sessionId
  }

  async function killSessionAction(sessionId: string) {
    console.log('killSession called with sessionId:', sessionId)
    await killSession(sessionId)
    sessions.value = await listSessions()
    console.log('killSession completed, sessions:', sessions.value.map(s => ({ id: s.id, status: s.status })))

    if (activeSession.value?.id === sessionId) {
      activeSession.value = null
    }
  }

  async function deleteSessionAction(sessionId: string) {
    console.log('deleteSession called with sessionId:', sessionId)
    await deleteSession(sessionId)
    sessions.value = await listSessions()

    if (activeSession.value?.id === sessionId) {
      activeSession.value = null
    }
  }

  async function restartSessionAction(sessionId: string) {
    console.log('restartSession called with sessionId:', sessionId)
    await restartSession(sessionId)
    sessions.value = await listSessions()

    // Find the restarted session (should have same name but new id)
    const session = sessions.value.find(s => s.id === sessionId || s.name === sessions.value.find(s2 => s2.id === sessionId)?.name)
    if (session) {
      activeSession.value = session
    }

    return sessionId
  }

  async function createConfig(
    name: string,
    environment: string,
    workingDir: string,
    command: string,
    wslDistro?: string,
    tmuxSession?: string
  ) {
    console.log('[session store] createConfig called:', { name, environment, workingDir, command })
    try {
      const result = await createSessionConfig({
        name,
        environment,
        working_dir: workingDir,
        command,
        wsl_distro: wslDistro,
        tmux_session: tmuxSession,
      })
      console.log('[session store] createSessionConfig returned:', result)
      configs.value = await listSessionConfigs()
      console.log('[session store] configs refreshed:', configs.value.length)
    } catch (e: any) {
      console.error('[session store] createConfig error:', e)
      console.error('[session store] error message:', e?.message)
      console.error('[session store] error stack:', e?.stack)
      throw e
    }
  }

  async function deleteConfigAction(id: string) {
    await deleteSessionConfig(id)
    configs.value = await listSessionConfigs()
  }

  async function updateConfigAction(
    id: string,
    name: string,
    environment: string,
    workingDir: string,
    command: string,
    wslDistro?: string,
    tmuxSession?: string,
    autoStart?: boolean
  ) {
    await updateSessionConfig({
      id,
      name,
      environment,
      working_dir: workingDir || '',
      command: command || '',
      wsl_distro: wslDistro,
      tmux_session: tmuxSession,
      auto_start: autoStart,
    })
    configs.value = await listSessionConfigs()
  }

  async function writeToSessionAction(sessionId: string, data: string) {
    await writeToSession(sessionId, data)
  }

  async function sendSpecialKeyAction(sessionId: string, key: string) {
    await sendSpecialKey(sessionId, key)
  }

  async function resizeSessionAction(sessionId: string, cols: number, rows: number) {
    await resizeSession(sessionId, cols, rows)
  }

  return {
    sessions,
    configs,
    activeSession,
    loadConfigs,
    loadSessions,
    createSession,
    killSession: killSessionAction,
    deleteSession: deleteSessionAction,
    restartSession: restartSessionAction,
    createConfig,
    deleteConfig: deleteConfigAction,
    updateConfig: updateConfigAction,
    writeToSession: writeToSessionAction,
    sendSpecialKey: sendSpecialKeyAction,
    resizeSession: resizeSessionAction,
  }
})