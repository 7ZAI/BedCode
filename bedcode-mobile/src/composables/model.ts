//! Shared Composable Types

export interface ToastOptions {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
  duration?: number
  position?: 'top' | 'bottom'
}

export interface PlatformInfo {
  platform: 'windows' | 'macos' | 'linux' | 'android' | 'ios' | null
  arch: 'x86_64' | 'aarch64' | 'arm' | null
  osVersion: string | null
  osType: string | null
  isDesktop: boolean
  isMobile: boolean
  isWindows: boolean
  isMacos: boolean
  isLinux: boolean
  isAndroid: boolean
  isIos: boolean
}

// ==================== Connection Types ====================

export type ConnectionStatus =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'pairing'
  | 'paired'
  | 'error'

/** 远程设备信息 */
export interface RemoteDevice {
  id: string
  name: string
  address: string
  port: number
  isPaired: boolean
  fingerprint?: string
}

/** 认证凭据 */
export interface AuthCredentials {
  pairingId: string
  fingerprint: string
  sessionToken: string
}

/** 连接信息 */
export interface ConnectionInfo {
  address: string
  port: number
  status: string
}

/** 认证状态 */
export interface AuthState {
  status: string
  is_authenticated: boolean
}

// ==================== Session Types ====================

/** 会话信息 */
export interface SessionInfo {
  id: string
  name: string
  config_id: string
  status: string
  created_at: string
}

/** 远程会话 */
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
  taskStatus?: string
  taskReason?: string
}

// ==================== Terminal Types ====================

/** 终端输出事件 */
export interface TerminalOutputEvent {
  session_id: string
  data: string
  is_waiting: boolean
  index: number
  timestamp: number
}

/** 终端增量输出 */
export interface TerminalIncrementalOutput {
  events: TerminalOutputEvent[]
  current_index: number
  is_initial: boolean
}

// ==================== Preset Task Types ====================

/** 预设任务执行状态（见 presetTaskState） */
export type PresetTaskStatus = 'unused' | 'executing' | 'completed' | 'interrupted'

/** 预设任务 */
export interface PresetTask {
  id: string
  content: string
  createdAt: string
  updatedAt: string
  /** 可重复：true 不受执行历史限制可反复入队；false 一旦执行过即锁定 */
  repeatable: boolean
  /** 执行状态（本地记录，入队即视为已执行） */
  status: PresetTaskStatus
  /** 入队时记录的队列项 id，用于完成广播/移除事件匹配 */
  pendingTaskId: string | null
  /** 入队时所在会话 id，对账仅对同会话的预设做判定（防多会话误中断） */
  pendingSessionId: string | null
}
