// ==================== Desktop Types ====================

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

// ==================== Shared Types ====================

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
