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
  fingerprint?: string  // 设备指纹，用于识别同一设备
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
  /** 任务执行状态（Plugin 会话使用） */
  taskStatus?: string
  /** 任务状态原因 */
  taskReason?: string
}

export interface TerminalOutputEvent {
  session_id: string
  data: string
  is_waiting: boolean
  index: number
  timestamp: number
}

export interface TerminalIncrementalOutput {
  events: TerminalOutputEvent[]
  current_index: number
  is_initial: boolean
}

/** 预设任务类型 */
export type PresetTaskType = 'once' | 'template'

/** 一次性任务状态 */
export type OnceTaskStatus = 'pending' | 'running' | 'completed' | 'failed'

/** 预设任务 */
export interface PresetTask {
  id: string
  title: string
  content: string
  type: PresetTaskType
  status: OnceTaskStatus | null
  createdAt: string
  updatedAt: string
}

