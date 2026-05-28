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

// Error Handler
export interface AppError {
  code: string
  message: string
  details?: string
  timestamp?: number
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

// Output Buffer
export interface BufferedOutput {
  sessionId?: string
  text?: string
  data?: string
  timestamp: number
}

// Output Parser
export interface OutputBlock {
  id: string
  type: 'text' | 'code' | 'link' | 'error' | 'success' | 'info' | 'warning' | 'markdown' | 'tool_use' | 'progress'
  content: string
  raw?: string
  language?: string
  timestamp: number
}

// Plugin Session
export interface PluginSessionInfo {
  id: string
  name: string
  config_id: string
  status: string
  session_type: string
  created_at: string
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

// Device Store
export interface PairedDevice {
  id: string
  name: string
  address: string
  port: number
}

// Notification
export interface Notification {
  id: string
  type: 'info' | 'success' | 'warning' | 'error'
  title: string
  message: string
  duration?: number
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

export interface ToastOptions {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
  duration?: number
  position?: 'top' | 'bottom'
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
}