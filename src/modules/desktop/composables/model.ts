// ==================== Desktop Types ====================

export interface WslDistro {
  name: string
  state: string
}

export interface TmuxSession {
  name: string
  windows: number
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
  tmux_session?: string
  tmuxSession?: string
  auto_start?: boolean
  autoStart?: boolean
}

export interface DeviceConnectionInfo {
  addr: string
  device_id: string
  session_count: number
}

export interface PtyOutputEvent {
  sessionId: string
  data: string
  timestamp: string
  isWaiting: boolean
  index: number
}

