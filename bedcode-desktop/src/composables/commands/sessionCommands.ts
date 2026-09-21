/**
 * 会话域命令封装（useDesktopCommands 拆分产物）
 *
 * WSL 探测 + 会话生命周期 + 会话配置 CRUD。统一收敛 Tauri 命令调用，
 * 超时敏感的启动路径走 invokeWithTimeout（含超时语义）。
 *
 * 保留范围（2026-09-21 收敛）：只保留**宿主页面仍直接调用**的封装
 * （终端窗口 / 会话兜底壳 / 设置页会话分组 / 插件 API 面 `context.sessions`）。
 * 原 `startSession`（按 configId 创建即启动）无调用方已删；配对 / QR / 连接历史 /
 * 快捷指令一族封装（`commands/deviceCommands.ts`）随业务域下沉插件已整文件删除。
 */
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
import { invokeWithTimeout } from '@/utils/invoke'
import type { WslDistro, SessionInfo, SessionConfig } from '../model'

export type { WslDistro, SessionInfo, SessionConfig }

// ==================== WSL Commands ====================

/** 获取已安装的 WSL 发行版列表 */
export async function listWslDistributions(): Promise<WslDistro[]> {
  return invoke('list_wsl_distributions')
}

/** 检查 WSL 是否可用 */
export async function isWslAvailable(): Promise<boolean> {
  return invoke('is_wsl_available')
}

// ==================== Session Commands ====================

/** 终端网格尺寸（启动时作为 PTY 初始 cols/rows） */
export interface TerminalSize {
  cols: number
  rows: number
}

/**
 * 创建会话但不启动 PTY（含超时）
 * 返回 sessionId，前端准备好后可调用 startExistingSession 启动
 */
export async function createSessionNoStart(configId: string): Promise<string> {
  return invokeWithTimeout('create_session_no_start', { configId })
}

/**
 * 启动已存在的会话（含超时，用于延迟启动场景）
 *
 * size：spawn 前按该尺寸调整 PTY（两阶段启动的第二阶段传入）
 */
export async function startExistingSession(sessionId: string, size?: TerminalSize): Promise<void> {
  return invokeWithTimeout('start_existing_session', {
    sessionId,
    cols: size?.cols,
    rows: size?.rows,
  })
}

/** 获取会话列表 */
export async function listSessions(): Promise<SessionInfo[]> {
  return invoke('list_sessions')
}

/** 获取单个会话信息 */
export async function getSession(sessionId: string): Promise<SessionInfo | null> {
  return invoke('get_session', { sessionId })
}

/** 终止会话 */
export async function killSession(sessionId: string): Promise<void> {
  return invoke('kill_session', { sessionId })
}

/** 删除会话 */
export async function deleteSession(sessionId: string): Promise<void> {
  return invoke('delete_session', { sessionId })
}

/** 重启会话 */
export async function restartSession(sessionId: string): Promise<void> {
  return invoke('restart_session', { sessionId })
}

/** 渲染端来源：桌面端 / 移动端设备（resize 裁决展示用） */
export type RendererSource = { kind: 'desktop' } | { kind: 'mobile'; deviceName: string }

/** resize 裁决结果（与 Rust 侧 ResizeOutcome serde 形状对齐） */
export type ResizeOutcome =
  | { status: 'applied'; canonical: RendererSource }
  | { status: 'needsConfirmation'; currentCanonical: RendererSource }

/**
 * 调整会话终端大小（正统渲染端裁决）
 *
 * force=false：本端非当前正统渲染端时返回 needsConfirmation（未应用），
 * 由调用方弹窗确认后以 force=true 重发覆盖。
 */
export async function resizeSession(
  sessionId: string,
  cols: number,
  rows: number,
  force = false,
): Promise<ResizeOutcome> {
  return invoke('resize_session', { sessionId, cols, rows, force })
}

/** 发送输入到会话 */
export async function writeToSession(sessionId: string, data: string): Promise<void> {
  return invoke('write_to_session', { sessionId, data })
}

/** 发送特殊键 */
export async function sendSpecialKey(sessionId: string, key: string): Promise<void> {
  return invoke('send_special_key', { sessionId, key })
}

// ==================== Config Commands ====================

/** 创建会话配置 */
export async function createSessionConfig(config: {
  name: string
  environment: string
  working_dir?: string
  command?: string
  wsl_distro?: string
}): Promise<SessionConfig> {
  logger.log('[createSessionConfig] calling backend with:', {
    name: config.name,
    environment: config.environment,
    working_dir: config.working_dir || '',
    command: config.command || '',
    wsl_distro: config.wsl_distro,
  })

  const result = await invoke('create_session_config', {
    name: config.name,
    environment: config.environment,
    working_dir: config.working_dir || '',
    command: config.command || '',
    wsl_distro: config.wsl_distro,
  })

  logger.log('[createSessionConfig] backend returned:', result)
  return result as SessionConfig
}

/** 获取会话配置列表 */
export async function listSessionConfigs(): Promise<SessionConfig[]> {
  return invoke('list_session_configs')
}

/** 获取单个会话配置 */
export async function getSessionConfig(configId: string): Promise<SessionConfig | null> {
  return invoke('get_session_config', { id: configId })
}

/** 删除会话配置 */
export async function deleteSessionConfig(configId: string): Promise<void> {
  return invoke('delete_session_config', { id: configId })
}

/** 更新会话配置 */
export async function updateSessionConfig(config: {
  id: string
  name: string
  environment: string
  working_dir: string
  command: string
  wsl_distro?: string
  auto_start?: boolean
}): Promise<void> {
  logger.log('[updateSessionConfig] calling with:', config)
  return invoke('update_session_config', {
    id: config.id,
    name: config.name,
    environment: config.environment,
    working_dir: config.working_dir,
    command: config.command,
    wsl_distro: config.wsl_distro,
    auto_start: config.auto_start,
  })
}
