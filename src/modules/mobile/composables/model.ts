// ==================== Mobile Types ====================

export type ConnectionStatus =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'pairing'
  | 'paired'
  | 'error'

export interface RemoteDevice {
  id: string
  name: string
  address: string
  port: number
  isPaired: boolean
}

export interface AuthCredentials {
  pairingId: string
  fingerprint: string
  sessionToken: string
}

export interface ConnectionInfo {
  address: string
  port: number
  status: string
}

export interface AuthState {
  status: string
  is_authenticated: boolean
}

export interface SessionInfo {
  id: string
  name: string
  config_id: string
  status: string
  created_at: string
}

export interface RemoteSession {
  id: string
  name: string
  config_id: string
  configId?: string
  status: string
  session_type?: string
  sessionType?: string
  created_at: string
  createdAt?: string
  startedAt?: string
  stoppedAt?: string
  is_active: boolean
}

export interface TerminalOutputEvent {
  session_id: string
  data: string
  is_waiting: boolean
  index: number
  timestamp: number
}

export interface TerminalHistory {
  events: TerminalOutputEvent[]
  current_index: number
  total_count: number
}

export interface TerminalIncrementalOutput {
  events: TerminalOutputEvent[]
  current_index: number
  is_initial: boolean
}

