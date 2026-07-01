// ==================== Desktop Types ====================

export interface WslDistro {
  name: string
  state: string
}

export interface SessionInfo {
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
  /** 任务执行状态（Plugin 会话使用） */
  taskStatus?: string
  /** 任务状态原因 */
  taskReason?: string
}

export interface SessionConfig {
  id: string
  name: string
  environment: string
  wsl_distro?: string
  wslDistro?: string
  working_dir?: string
  workingDir?: string
  command?: string
  auto_start?: boolean
  autoStart?: boolean
}

export interface DeviceConnectionInfo {
  addr: string
  device_id: string
  /** 设备指纹，用于与数据库 pairings 记录关联匹配 */
  fingerprint?: string
  session_count: number
}

export interface PtyOutputEvent {
  sessionId: string
  data: string
  timestamp: string
  isWaiting: boolean
  index: number
}

// ==================== Shared Types ====================

// ANSI Renderer
export interface AnsiRenderOptions {
  backgroundColor?: string
  foregroundColor?: string
  fontFamily?: string
  fontSize?: number
  lineHeight?: number
  fontWeight?: string | number
  bold?: boolean
  italic?: boolean
  useClasses?: boolean
}

// Keyboard Shortcuts
export interface Shortcut {
  key: string
  ctrl?: boolean
  alt?: boolean
  shift?: boolean
  meta?: boolean
  action?: () => void
  handler?: () => void
  description?: string
  ignoreInput?: boolean
}

// QR Code
export interface QrConnectionInfo {
  url: string
  host: string
  port: number
  token: string
  /** 剩余有效时间（秒） */
  remaining_secs: number
}

// Session Status Events
export interface SessionStatusEvent {
  sessionId: string
  oldStatus: string | null
  newStatus: string
  sessionName: string
}

export interface SessionRestartEvent {
  oldSessionId: string
  newSessionId: string
  sessionName: string
}

export interface SessionEventPayload {
  type: string
  event_type?: string
  session?: { id: string; name: string; status: string }
  device_name?: string
}

export interface DeviceEventPayload {
  addr?: string
  device_id?: string
  device_name?: string
  event?: string
}

export interface TerminalWindowState {
  window: any
  isSnapped: boolean
  snapDirection: 'left' | 'right' | null
  lastPosition: { x: number; y: number }
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
