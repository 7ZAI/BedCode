//! Tauri API Composable
//!
//! Vue composable for calling Tauri backend commands

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Ref } from 'vue'
import { ref, onMounted, onUnmounted, watch } from 'vue'

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
  // 注意：后端使用 camelCase 序列化 enum，所以值是小写开头的
  status: 'starting' | 'running' | 'waitingInput' | 'stopped' | 'error'
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

export interface QrConnectionInfo {
  token: string
  host: string
  port: number
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

      if (!sid || event.payload.sessionId === sid) {
        const data = decodeBase64Utf8(event.payload.data)

        // 写入输出缓冲区（桌面端 xterm.js 通过 watcher 增量读取）
        // 注意：不清空/裁剪数组，否则 TerminalPreview 的 lastOutputIndex 会失效
        output.value.push(data)

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

  // 当 sessionId 变化时清空输出（切换会话）
  if (typeof sessionId !== 'string') {
    watch(sessionId, (newSid, oldSid) => {
      if (newSid !== oldSid && oldSid !== undefined) {
        console.log('[PTY] Session changed from', oldSid, 'to', newSid, 'clearing output')
        clearOutput()
      }
    })
  }

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

// QR Code Commands
export function useQrCodeApi() {
  async function generateQrCode(): Promise<string> {
    try {
      return await invoke<string>('generate_qr_code')
    } catch (e) {
      console.error('Failed to generate QR code:', e)
      throw e
    }
  }

  async function clearQrCode(): Promise<void> {
    try {
      await invoke('clear_qr_code')
    } catch (e) {
      console.error('Failed to clear QR code:', e)
    }
  }

  async function getQrConnectionInfo(): Promise<QrConnectionInfo | null> {
    try {
      return await invoke<QrConnectionInfo | null>('get_qr_connection_info')
    } catch (e) {
      console.error('Failed to get QR connection info:', e)
      return null
    }
  }

  async function getQrTokenTtl(): Promise<number> {
    try {
      return await invoke<number>('get_qr_token_ttl')
    } catch (e) {
      console.error('Failed to get QR token TTL:', e)
      return 300
    }
  }

  async function setQrTokenTtl(seconds: number): Promise<void> {
    try {
      await invoke('set_qr_token_ttl', { seconds })
    } catch (e) {
      console.error('Failed to set QR token TTL:', e)
    }
  }

  return {
    generateQrCode,
    clearQrCode,
    getQrConnectionInfo,
    getQrTokenTtl,
    setQrTokenTtl,
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

// Connected Devices (WebSocket clients)
export interface DeviceConnectionInfo {
  addr: string
  device_id: string
  session_count: number
}

export function useConnectedDevices() {
  const connectedDevices = ref<DeviceConnectionInfo[]>([])
  const isLoading = ref(false)

  async function loadConnectedDevices() {
    isLoading.value = true
    try {
      connectedDevices.value = await invoke<DeviceConnectionInfo[]>('get_connected_devices')
    } catch (e) {
      console.error('Failed to load connected devices:', e)
    } finally {
      isLoading.value = false
    }
  }

  return { connectedDevices, isLoading, loadConnectedDevices }
}

// Utility functions

/**
 * Base64 解码为 UTF-8 字符串
 * atob() 无法正确处理多字节 UTF-8 字符，需要使用 TextDecoder
 */
function decodeBase64Utf8(base64: string): string {
  const binary = atob(base64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return new TextDecoder('utf-8').decode(bytes)
}

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
