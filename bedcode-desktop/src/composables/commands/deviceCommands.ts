/**
 * 设备 / 配对 / 历史 / QR / 快捷操作命令封装（useDesktopCommands 拆分产物）
 */
import { invoke } from '@tauri-apps/api/core'
import type { DeviceConnectionInfo } from '../model'

export type { DeviceConnectionInfo }

/** 配对码信息 */
export interface PairingCodeInfo {
  code: string
  created_at: string
  expires_in: number
}

// ==================== Device Commands ====================

/** 获取已连接的移动设备列表 */
export async function getConnectedDevices(): Promise<DeviceConnectionInfo[]> {
  return invoke('get_connected_devices')
}

// ==================== Pairing Commands ====================

/** 生成配对码（返回完整的配对码信息：code、创建时间、有效期） */
export async function generatePairingCode(): Promise<PairingCodeInfo> {
  return invoke('generate_pairing_code')
}

/** 获取配对码有效期（秒） */
export async function getPairingCodeTtl(): Promise<number> {
  return invoke('get_pairing_code_ttl')
}

/** 设置配对码有效期（秒） */
export async function setPairingCodeTtl(ttl: number): Promise<void> {
  return invoke('set_pairing_code_ttl', { ttl })
}

/** 获取当前配对码 */
export async function getCurrentPairingCode(): Promise<PairingCodeInfo | null> {
  return invoke('get_current_pairing_code')
}

/** 验证配对码 */
export async function verifyPairingCode(code: string): Promise<boolean> {
  return invoke('verify_pairing_code', { code })
}

/** 清除配对码 */
export async function clearPairingCode(): Promise<void> {
  return invoke('clear_pairing_code')
}

/** 获取已配对设备列表 */
export async function listPairedDevices(): Promise<any[]> {
  return invoke('list_paired_devices')
}

/** 移除已配对设备 */
export async function removePairedDevice(deviceId: string): Promise<void> {
  return invoke('remove_paired_device', { id: deviceId })
}

// ==================== Connection History Commands ====================

/** 设备连接历史条目（与后端 ConnectionHistory 序列化字段对应） */
export interface ConnectionHistoryEntry {
  id: number
  deviceId: string
  authMethod: string
  result: string
  address: string | null
  connectedAt: string
  disconnectedAt: string | null
}

/** 获取设备连接历史 */
export async function listConnectionHistory(deviceId: string): Promise<ConnectionHistoryEntry[]> {
  return invoke('list_connection_history', { deviceId })
}

/** 删除设备连接历史 */
export async function deleteConnectionHistory(deviceId: string): Promise<void> {
  return invoke('delete_connection_history', { deviceId })
}

// ==================== QR Commands ====================

/** 生成二维码连接信息 */
export async function generateQrCode(): Promise<string> {
  return invoke('generate_qr_code')
}

/** 清除二维码 */
export async function clearQrCode(): Promise<void> {
  return invoke('clear_qr_code')
}

/** 获取二维码连接信息 */
export async function getQrConnectionInfo(host?: string): Promise<any> {
  return invoke('get_qr_connection_info', { host })
}

/** 获取 QR Token TTL */
export async function getQrTokenTtl(): Promise<number> {
  return invoke('get_qr_token_ttl')
}

/** 设置 QR Token TTL */
export async function setQrTokenTtl(ttl: number): Promise<void> {
  return invoke('set_qr_token_ttl', { ttl })
}

// ==================== Quick Actions ====================

/** 获取快捷操作列表 */
export async function listQuickActions(): Promise<any[]> {
  return invoke('list_quick_actions')
}

/** 创建快捷操作 */
export async function createQuickAction(action: any): Promise<string> {
  return invoke('create_quick_action', { action })
}

/** 更新快捷操作 */
export async function updateQuickAction(action: any): Promise<void> {
  return invoke('update_quick_action', { action })
}

/** 删除快捷操作 */
export async function deleteQuickAction(actionId: string): Promise<void> {
  return invoke('delete_quick_action', { actionId })
}
