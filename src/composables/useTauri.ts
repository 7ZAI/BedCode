//! Tauri API Composable
//!
//! Vue composable for calling Tauri backend commands

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Ref } from 'vue'
import { ref, onMounted, onUnmounted } from 'vue'

// Types
export interface SessionConfig {
  id: string
  name: string
  environment: 'windows' | 'wsl2'
  wslDistro?: string
  workingDir: string
  command: string
  tmuxSession?: string
  autoStart: boolean
  createdAt: string
  updatedAt: string
}

export interface SessionInfo {
  id: string
  configId: string
  name: string
  status: 'Starting' | 'Running' | 'WaitingInput' | 'Stopped' | 'Error'
  createdAt: string
  startedAt?: string
  stoppedAt?: string
}

export interface PtyOutputEvent {
  sessionId: string
  data: string // Base64 encoded
  timestamp: string
}

export interface WslDistro {
  name: string
  isDefault: boolean
  state: string
  version: number
}

export interface TmuxSession {
  name: string
  windows: number
  isAttached: boolean
  created?: string
}

export interface QuickAction {
  id: string
  name: string
  content: string
  icon?: string
  color?: string
  sortOrder: number
  createdAt: string
}

export interface Pairing {
  id: string
  deviceName: string
  deviceFingerprint: string
  publicKey: string
  pairedAt: string
  lastSeen?: string
  isActive: boolean
}

export interface PairingCode {
  code: string
  expiresIn: number
}

export interface DiscoveredDevice {
  name: string
  address: string
  port: number
  properties: Record<string, string>
  discoveredAt: string
}

// WSL Commands
export function useWsl() {
  const distros = ref<WslDistro[]>([])
  const isAvailable = ref(false)

  async function loadDistros() {
    try {
      isAvailable.value = await invoke('is_wsl_available')
      if (isAvailable.value) {
        distros.value = await invoke('list_wsl_distributions')
      }
    } catch (e) {
      console.error('Failed to load WSL distros:', e)
    }
  }

  return { distros, isAvailable, loadDistros }
}

// Tmux Commands
export function useTmux() {
  const sessions = ref<TmuxSession[]>([])
  const isAvailable = ref(false)

  async function loadSessions() {
    try {
      isAvailable.value = await invoke('is_tmux_available')
      if (isAvailable.value) {
        sessions.value = await invoke('list_tmux_sessions')
      }
    } catch (e) {
      console.error('Failed to load tmux sessions:', e)
    }
  }

  async function createSession(name: string, command?: string) {
    await invoke('create_tmux_session', { name, command })
    await loadSessions()
  }

  return { sessions, isAvailable, loadSessions, createSession }
}

// Session Config Commands
export function useSessionConfig() {
  const configs = ref<SessionConfig[]>([])

  async function loadConfigs() {
    try {
      configs.value = await invoke('list_session_configs')
    } catch (e) {
      console.error('Failed to load session configs:', e)
    }
  }

  async function createConfig(
    name: string,
    environment: string,
    workingDir: string,
    command: string,
    wslDistro?: string,
    tmuxSession?: string
  ): Promise<SessionConfig> {
    const config = await invoke('create_session_config', {
      name,
      environment,
      workingDir,
      command,
      wslDistro,
      tmuxSession,
    })
    await loadConfigs()
    return config as SessionConfig
  }

  async function deleteConfig(id: string) {
    await invoke('delete_session_config', { id })
    await loadConfigs()
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
  ): Promise<SessionConfig> {
    const config = await invoke('update_session_config', {
      id,
      name,
      environment,
      workingDir,
      command,
      wslDistro,
      tmuxSession,
      autoStart,
    })
    await loadConfigs()
    return config as SessionConfig
  }

  return { configs, loadConfigs, createConfig, deleteConfig, updateConfig }
}

// Session Commands
export function useSession() {
  const sessions = ref<SessionInfo[]>([])
  const outputs = ref<Map<string, string[]>>(new Map())

  async function loadSessions() {
    try {
      sessions.value = await invoke('list_sessions')
    } catch (e) {
      console.error('Failed to load sessions:', e)
    }
  }

  async function startSession(configId: string): Promise<string> {
    const sessionId = await invoke('start_session', { configId })
    await loadSessions()
    return sessionId as string
  }

  async function killSession(sessionId: string) {
    await invoke('kill_session', { sessionId })
    await loadSessions()
  }

  async function writeToSession(sessionId: string, data: string) {
    await invoke('write_to_session', { sessionId, data })
  }

  async function sendSpecialKey(sessionId: string, key: string) {
    await invoke('send_special_key', { sessionId, key })
  }

  async function resizeSession(sessionId: string, cols: number, rows: number) {
    await invoke('resize_session', { sessionId, cols, rows })
  }

  return {
    sessions,
    outputs,
    loadSessions,
    startSession,
    killSession,
    writeToSession,
    sendSpecialKey,
    resizeSession,
  }
}

// PTY Output Listener
export function usePtyOutput(sessionId: string | Ref<string>) {
  const output = ref<string[]>([])
  const isWaiting = ref(false)
  let unlisten: (() => void) | null = null

  async function startListening() {
    unlisten = await listen<PtyOutputEvent>('pty-output', (event) => {
      const sid = typeof sessionId === 'string' ? sessionId : sessionId.value
      if (event.payload.sessionId === sid) {
        // Decode base64
        const data = atob(event.payload.data)
        output.value.push(data)

        // Limit output buffer
        if (output.value.length > 1000) {
          output.value = output.value.slice(-500)
        }

        // Detect waiting input
        isWaiting.value = detectWaitingInput(data)
      }
    })
  }

  function stopListening() {
    if (unlisten) {
      unlisten()
      unlisten = null
    }
  }

  function clearOutput() {
    output.value = []
  }

  onMounted(() => {
    startListening()
  })

  onUnmounted(() => {
    stopListening()
  })

  return { output, isWaiting, clearOutput, startListening, stopListening }
}

// Quick Actions
export function useQuickActions() {
  const actions = ref<QuickAction[]>([])

  async function loadActions() {
    try {
      actions.value = await invoke('list_quick_actions')
    } catch (e) {
      console.error('Failed to load quick actions:', e)
    }
  }

  async function createAction(
    name: string,
    content: string,
    icon?: string,
    color?: string
  ): Promise<QuickAction> {
    const action = await invoke('create_quick_action', { name, content, icon, color })
    await loadActions()
    return action as QuickAction
  }

  return { actions, loadActions, createAction }
}

// Pairing
export function usePairing() {
  const devices = ref<Pairing[]>([])
  const pairingCode = ref<PairingCode | null>(null)

  async function loadDevices() {
    try {
      devices.value = await invoke('list_paired_devices')
    } catch (e) {
      console.error('Failed to load paired devices:', e)
    }
  }

  async function generateCode() {
    try {
      pairingCode.value = await invoke('generate_pairing_code')
    } catch (e) {
      console.error('Failed to generate pairing code:', e)
    }
  }

  async function verifyCode(code: string): Promise<boolean> {
    try {
      return await invoke('verify_pairing_code', { code })
    } catch (e) {
      console.error('Failed to verify pairing code:', e)
      return false
    }
  }

  async function clearCode() {
    try {
      await invoke('clear_pairing_code')
      pairingCode.value = null
    } catch (e) {
      console.error('Failed to clear pairing code:', e)
    }
  }

  async function removeDevice(id: string) {
    await invoke('remove_paired_device', { id })
    await loadDevices()
  }

  return { devices, pairingCode, loadDevices, generateCode, verifyCode, clearCode, removeDevice }
}

// Discovery
export function useDiscovery() {
  const discoveredDevices = ref<DiscoveredDevice[]>([])

  async function startDiscovery() {
    try {
      await invoke('start_discovery')
    } catch (e) {
      console.error('Failed to start discovery:', e)
    }
  }

  async function loadDiscoveredDevices() {
    try {
      discoveredDevices.value = await invoke('get_discovered_devices')
    } catch (e) {
      console.error('Failed to load discovered devices:', e)
    }
  }

  async function startBroadcast(serviceName: string, port: number) {
    try {
      await invoke('start_broadcast', { serviceName, port })
    } catch (e) {
      console.error('Failed to start broadcast:', e)
    }
  }

  return {
    discoveredDevices,
    startDiscovery,
    loadDiscoveredDevices,
    startBroadcast,
  }
}

// Network utilities
export function useNetwork() {
  const localAddresses = ref<string[]>([])

  async function loadLocalAddresses() {
    try {
      localAddresses.value = await invoke('get_local_ip_addresses')
    } catch (e) {
      console.error('Failed to get local IP addresses:', e)
    }
  }

  return { localAddresses, loadLocalAddresses }
}

// Utility functions
function detectWaitingInput(text: string): boolean {
  const patterns = [
    /> $/, // Claude Code default
    /❯ $/, // Some shells
    /\?\s*$/, // Question ending
    /\[Y\/n\]\s*$/, // Confirmation prompt
    /press any key/i, // Key press prompt
  ]

  return patterns.some((p) => p.test(text))
}
