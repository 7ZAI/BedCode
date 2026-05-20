//! Desktop Commands - Rust 后端命令封装
//!
//! 所有桌面端可用的 Tauri 命令调用

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

import type { WslDistro, TmuxSession, SessionInfo, SessionConfig, DeviceConnectionInfo } from './model'
export type { WslDistro, TmuxSession, SessionInfo, SessionConfig, DeviceConnectionInfo }

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

// ==================== Tmux Commands ====================

/**
 * 获取 Tmux 会话列表
 */
export async function listTmuxSessions(): Promise<TmuxSession[]> {
  return await invoke('list_tmux_sessions')
}

/**
 * 检查 Tmux 是否可用
 */
export async function isTmuxAvailable(): Promise<boolean> {
  return await invoke('is_tmux_available')
}

/**
 * 创建 Tmux 会话
 */
export async function createTmuxSession(name: string, command?: string): Promise<void> {
  return await invoke('create_tmux_session', { name, command })
}

// ==================== Session Commands ====================

/**
 * 启动会话
 */
export async function startSession(configId: string): Promise<string> {
  return await invoke('start_session', { configId })
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
export async function createSessionConfig(config: Omit<SessionConfig, 'id'>): Promise<string> {
  return await invoke('create_session_config', { config })
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
  return await invoke('get_session_config', { configId })
}

/**
 * 删除会话配置
 */
export async function deleteSessionConfig(configId: string): Promise<void> {
  return await invoke('delete_session_config', { configId })
}

/**
 * 更新会话配置
 */
export async function updateSessionConfig(config: SessionConfig): Promise<void> {
  return await invoke('update_session_config', { config })
}

// ==================== Pairing Commands ====================

/**
 * 生成配对码
 */
export async function generatePairingCode(): Promise<string> {
  return await invoke('generate_pairing_code')
}

/**
 * 获取当前配对码
 */
export async function getCurrentPairingCode(): Promise<string | null> {
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
  return await invoke('remove_paired_device', { deviceId })
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
  unlistenPtyOutput = await listen('pty-output', callback)
  return unlistenPtyOutput
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

    // Tmux
    listTmuxSessions,
    isTmuxAvailable,
    createTmuxSession,

    // Session
    startSession,
    listSessions,
    getSession,
    killSession,
    deleteSession,
    restartSession,
    resizeSession,
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