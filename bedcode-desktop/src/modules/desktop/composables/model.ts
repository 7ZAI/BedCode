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

