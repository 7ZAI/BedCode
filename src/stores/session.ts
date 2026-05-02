import { defineStore } from 'pinia'
import { ref } from 'vue'
import {
  useSessionConfig as useConfigApi,
  useSession as useSessionApi,
  type SessionConfig,
  type SessionInfo
} from '@/composables/useTauri'

export { type SessionConfig, type SessionInfo }

export const useSessionStore = defineStore('session', () => {
  const sessions = ref<SessionInfo[]>([])
  const configs = ref<SessionConfig[]>([])
  const activeSession = ref<SessionInfo | null>(null)

  const configApi = useConfigApi()
  const sessionApi = useSessionApi()

  async function loadConfigs() {
    await configApi.loadConfigs()
    configs.value = configApi.configs.value
  }

  async function loadSessions() {
    await sessionApi.loadSessions()
    sessions.value = sessionApi.sessions.value
  }

  async function createSession(configId: string) {
    const sessionId = await sessionApi.startSession(configId)
    sessions.value = sessionApi.sessions.value

    // Find the new session and set as active
    const session = sessions.value.find(s => s.id === sessionId)
    if (session) {
      activeSession.value = session
    }

    return sessionId
  }

  async function killSession(sessionId: string) {
    await sessionApi.killSession(sessionId)
    sessions.value = sessionApi.sessions.value

    if (activeSession.value?.id === sessionId) {
      activeSession.value = null
    }
  }

  async function createConfig(
    name: string,
    environment: string,
    workingDir: string,
    command: string,
    wslDistro?: string,
    tmuxSession?: string
  ) {
    await configApi.createConfig(name, environment, workingDir, command, wslDistro, tmuxSession)
    configs.value = configApi.configs.value
  }

  async function deleteConfig(id: string) {
    await configApi.deleteConfig(id)
    configs.value = configApi.configs.value
  }

  async function updateConfig(
    id: string,
    name: string,
    environment: string,
    workingDir: string,
    command: string,
    wslDistro?: string,
    tmuxSession?: string,
    autoStart?: boolean
  ) {
    await configApi.updateConfig(id, name, environment, workingDir, command, wslDistro, tmuxSession, autoStart)
    configs.value = configApi.configs.value
  }

  async function writeToSession(sessionId: string, data: string) {
    await sessionApi.writeToSession(sessionId, data)
  }

  async function sendSpecialKey(sessionId: string, key: string) {
    await sessionApi.sendSpecialKey(sessionId, key)
  }

  return {
    sessions,
    configs,
    activeSession,
    loadConfigs,
    loadSessions,
    createSession,
    killSession,
    createConfig,
    deleteConfig,
    updateConfig,
    writeToSession,
    sendSpecialKey,
  }
})
