//! Desktop Commands - Rust 后端命令封装
//!
//! 所有桌面端可用的 Tauri 命令调用

import { invoke } from '@tauri-apps/api/core'
import { invokeWithTimeout } from '@/utils/invoke'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

import type { WslDistro, SessionInfo, SessionConfig, DeviceConnectionInfo, PtyOutputEvent } from './model'
export type { WslDistro, SessionInfo, SessionConfig, DeviceConnectionInfo, PtyOutputEvent }

// ==================== Pairing Types ====================

/**
 * 配对码信息
 */
export interface PairingCodeInfo {
  code: string
  created_at: string
  expires_in: number
}

// ==================== WSL Commands ====================

/**
 * 获取已安装的 WSL 发行版列表
 */
export async function listWslDistributions(): Promise<WslDistro[]> {
  return await invoke('list_wsl_distributions')
}

/**
 * 检查 WSL 是否可用
 */
export async function isWslAvailable(): Promise<boolean> {
  return await invoke('is_wsl_available')
}

// ==================== Session Commands ====================

/**
 * 启动会话（含超时，PTY 进程创建可能耗时较长）
 */
export async function startSession(configId: string): Promise<string> {
  return await invokeWithTimeout('start_session', { configId })
}

/**
 * 创建会话但不启动 PTY（含超时）
 * 返回 sessionId，前端准备好后可调用 startExistingSession 启动
 */
export async function createSessionNoStart(configId: string): Promise<string> {
  return await invokeWithTimeout('create_session_no_start', { configId })
}

/**
 * 启动已存在的会话（含超时，用于延迟启动场景）
 */
export async function startExistingSession(sessionId: string): Promise<void> {
  return await invokeWithTimeout('start_existing_session', { sessionId })
}

/**
 * 获取会话列表
 */
export async function listSessions(): Promise<SessionInfo[]> {
  return await invoke('list_sessions')
}

/**
 * 获取单个会话信息
 */
export async function getSession(sessionId: string): Promise<SessionInfo | null> {
  return await invoke('get_session', { sessionId })
}

/**
 * 终止会话
 */
export async function killSession(sessionId: string): Promise<void> {
  return await invoke('kill_session', { sessionId })
}

/**
 * 删除会话
 */
export async function deleteSession(sessionId: string): Promise<void> {
  return await invoke('delete_session', { sessionId })
}

/**
 * 重启会话
 */
export async function restartSession(sessionId: string): Promise<void> {
  return await invoke('restart_session', { sessionId })
}

/**
 * 调整终端大小
 */
export async function resizeSession(sessionId: string, cols: number, rows: number): Promise<void> {
  return await invoke('resize_session', { sessionId, cols, rows })
}

/**
 * 获取会话的历史输出（用于回放）
 */
export async function getSessionOutputHistory(sessionId: string): Promise<PtyOutputEvent[]> {
  return await invoke('get_session_output_history', { sessionId })
}

/**
 * 发送输入到会话
 */
export async function writeToSession(sessionId: string, data: string): Promise<void> {
  return await invoke('write_to_session', { sessionId, data })
}

/**
 * 发送特殊键
 */
export async function sendSpecialKey(sessionId: string, key: string): Promise<void> {
  return await invoke('send_special_key', { sessionId, key })
}

// ==================== Device Commands ====================

/**
 * 获取已连接的移动设备列表
 */
export async function getConnectedDevices(): Promise<DeviceConnectionInfo[]> {
  return await invoke('get_connected_devices')
}

// ==================== Config Commands ====================

/**
 * 创建会话配置
 */
export async function createSessionConfig(config: {
  name: string
  environment: string
  working_dir?: string
  command?: string
  wsl_distro?: string
}): Promise<SessionConfig> {
  console.log('[createSessionConfig] calling backend with:', {
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

  console.log('[createSessionConfig] backend returned:', result)
  return result as SessionConfig
}

/**
 * 获取会话配置列表
 */
export async function listSessionConfigs(): Promise<SessionConfig[]> {
  return await invoke('list_session_configs')
}

/**
 * 获取单个会话配置
 */
export async function getSessionConfig(configId: string): Promise<SessionConfig | null> {
  return await invoke('get_session_config', { id: configId })
}

/**
 * 删除会话配置
 */
export async function deleteSessionConfig(configId: string): Promise<void> {
  return await invoke('delete_session_config', { id: configId })
}

/**
 * 更新会话配置
 */
export async function updateSessionConfig(config: {
  id: string
  name: string
  environment: string
  working_dir: string
  command: string
  wsl_distro?: string
  auto_start?: boolean
}): Promise<void> {
  console.log('[updateSessionConfig] calling with:', config)
  return await invoke('update_session_config', {
    id: config.id,
    name: config.name,
    environment: config.environment,
    working_dir: config.working_dir,
    command: config.command,
    wsl_distro: config.wsl_distro,
    auto_start: config.auto_start,
  })
}

// ==================== Pairing Commands ====================

/**
 * 生成配对码
 * 返回完整的配对码信息（包含 code、创建时间、有效期）
 */
export async function generatePairingCode(): Promise<PairingCodeInfo> {
  return await invoke('generate_pairing_code')
}

/**
 * 获取当前配对码
 */
export async function getCurrentPairingCode(): Promise<PairingCodeInfo | null> {
  return await invoke('get_current_pairing_code')
}

/**
 * 验证配对码
 */
export async function verifyPairingCode(code: string): Promise<boolean> {
  return await invoke('verify_pairing_code', { code })
}

/**
 * 清除配对码
 */
export async function clearPairingCode(): Promise<void> {
  return await invoke('clear_pairing_code')
}

/**
 * 获取已配对设备列表
 */
export async function listPairedDevices(): Promise<any[]> {
  return await invoke('list_paired_devices')
}

/**
 * 移除已配对设备
 */
export async function removePairedDevice(deviceId: string): Promise<void> {
  return await invoke('remove_paired_device', { id: deviceId })
}

// ==================== QR Commands ====================

/**
 * 生成二维码连接信息
 */
export async function generateQrCode(): Promise<string> {
  return await invoke('generate_qr_code')
}

/**
 * 清除二维码
 */
export async function clearQrCode(): Promise<void> {
  return await invoke('clear_qr_code')
}

/**
 * 获取二维码连接信息
 */
export async function getQrConnectionInfo(host?: string): Promise<any> {
  return await invoke('get_qr_connection_info', { host })
}

/**
 * 获取 QR Token TTL
 */
export async function getQrTokenTtl(): Promise<number> {
  return await invoke('get_qr_token_ttl')
}

/**
 * 设置 QR Token TTL
 */
export async function setQrTokenTtl(ttl: number): Promise<void> {
  return await invoke('set_qr_token_ttl', { ttl })
}

// ==================== Quick Actions ====================

/**
 * 获取快捷操作列表
 */
export async function listQuickActions(): Promise<any[]> {
  return await invoke('list_quick_actions')
}

/**
 * 创建快捷操作
 */
export async function createQuickAction(action: any): Promise<string> {
  return await invoke('create_quick_action', { action })
}

/**
 * 更新快捷操作
 */
export async function updateQuickAction(action: any): Promise<void> {
  return await invoke('update_quick_action', { action })
}

/**
 * 删除快捷操作
 */
export async function deleteQuickAction(actionId: string): Promise<void> {
  return await invoke('delete_quick_action', { actionId })
}

// ==================== Settings Commands ====================

/**
 * 获取所有数据库设置
 */
export async function getAllDbSettings(): Promise<Record<string, any>> {
  return await invoke('get_all_db_settings')
}

/**
 * 设置数据库项
 */
export async function setDbSetting(key: string, value: any): Promise<void> {
  return await invoke('set_db_setting', { key, value })
}

/**
 * 获取应用设置
 */
export async function getAppSettings(): Promise<any> {
  return await invoke('get_app_settings')
}

/**
 * 保存应用设置
 */
export async function saveAppSettings(settings: any): Promise<void> {
  return await invoke('save_app_settings', { settings })
}

// ==================== System Commands ====================

/**
 * Ping 命令，用于测试连接
 */
export async function ping(): Promise<string> {
  return await invoke('ping')
}

/**
 * 获取应用版本
 */
export async function getAppVersion(): Promise<string> {
  return await invoke('get_app_version')
}

/**
 * 获取应用启动时间
 */
export async function getStartupTime(): Promise<number> {
  return await invoke('get_startup_time')
}

/**
 * 获取本地 IP 地址列表
 */
export async function getLocalIpAddresses(): Promise<string[]> {
  return await invoke('get_local_ip_addresses')
}

// ==================== Event Listeners ====================

let unlistenDeviceConnected: UnlistenFn | null = null
let unlistenDeviceDisconnected: UnlistenFn | null = null
let unlistenPtyOutput: UnlistenFn | null = null

/**
 * 监听设备连接事件
 */
export async function onDeviceConnected(callback: (event: any) => void): Promise<() => void> {
  unlistenDeviceConnected = await listen('device-connected', callback)
  return unlistenDeviceConnected
}

/**
 * 监听设备断开事件
 */
export async function onDeviceDisconnected(callback: (event: any) => void): Promise<() => void> {
  unlistenDeviceDisconnected = await listen('device-disconnected', callback)
  return unlistenDeviceDisconnected
}

/**
 * 监听 PTY 输出事件
 */
export async function onPtyOutput(callback: (event: any) => void): Promise<() => void> {
  unlistenPtyOutput = await listen('pty-output', (event) => {
    callback(event.payload);
  });
  return unlistenPtyOutput;
}

/**
 * 清理所有事件监听
 */
export function cleanupEventListeners() {
  unlistenDeviceConnected?.()
  unlistenDeviceDisconnected?.()
  unlistenPtyOutput?.()
}

// ==================== Desktop Commands Composable ====================

/**
 * 桌面端命令 composable
 * 整合所有桌面端可用的 Rust 命令
 */
export function useDesktopCommands() {
  return {
    // WSL
    listWslDistributions,
    isWslAvailable,

    // Session
    startSession,
    createSessionNoStart,
    startExistingSession,
    listSessions,
    getSession,
    killSession,
    deleteSession,
    restartSession,
    resizeSession,
    getSessionOutputHistory,
    writeToSession,
    sendSpecialKey,

    // Device
    getConnectedDevices,

    // Config
    createSessionConfig,
    listSessionConfigs,
    getSessionConfig,
    deleteSessionConfig,
    updateSessionConfig,

    // Pairing
    generatePairingCode,
    getCurrentPairingCode,
    verifyPairingCode,
    clearPairingCode,
    listPairedDevices,
    removePairedDevice,

    // QR
    generateQrCode,
    clearQrCode,
    getQrConnectionInfo,
    getQrTokenTtl,
    setQrTokenTtl,

    // Quick Actions
    listQuickActions,
    createQuickAction,
    updateQuickAction,
    deleteQuickAction,

    // Settings
    getAllDbSettings,
    setDbSetting,
    getAppSettings,
    saveAppSettings,

    // System
    ping,
    getAppVersion,
    getStartupTime,
    getLocalIpAddresses,

    // Events
    onDeviceConnected,
    onDeviceDisconnected,
    onPtyOutput,
    cleanupEventListeners,
  }
}