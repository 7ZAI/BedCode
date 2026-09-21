/**
 * 会话域命令封装（useDesktopCommands 拆分产物）
 *
 * 只保留**宿主终端引擎直调**的封装（终端渲染管道红线）：会话记录读取、
 * 尺寸裁决、输入写入、特殊键。这些命令即使业务域已下沉插件，宿主终端窗口
 * 仍需直连通路（性能与背压门控都在内核 PTY 输出面）。
 *
 * 保留范围（2026-09-21 命令面收敛）：`list_sessions` / `get_session` /
 * `resize_session` / `write_to_session` / `send_special_key`。
 * 已注销（产品面归 `com.bedcode.session` 插件命令面）：
 * - 会话编排：`start_session` / `create_session_no_start` / `start_existing_session`
 *   / `kill_session` / `delete_session` / `restart_session`
 *   → 插件 `session.create` / `session.close` / `session.action.*`
 *   （宿主前端仅 `stores/session.ts` 的 `stopSession` 需要，经插件命令面转发）
 * - 会话配置 CRUD：`create/list/get/delete/update_session_config`
 *   → 插件 `session.config.list` / `.upsert` / `.delete`（插件私有库为配置真源）
 * - WSL 探测（`listWslDistributions` / `isWslAvailable`）
 *   → 插件 `session.environment.wsl-distros`
 * - 配对 / QR / 连接历史一族封装随宿主命令面注销整族删除
 */
import { invoke } from '@tauri-apps/api/core'
import type { SessionInfo } from '../model'

export type { SessionInfo }

// ==================== Session Commands ====================

/** 获取会话列表 */
export async function listSessions(): Promise<SessionInfo[]> {
  return invoke('list_sessions')
}

/** 获取单个会话信息 */
export async function getSession(sessionId: string): Promise<SessionInfo | null> {
  return invoke('get_session', { sessionId })
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
