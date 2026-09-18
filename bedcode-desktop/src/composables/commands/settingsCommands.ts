/**
 * 设置 / 系统命令封装（useDesktopCommands 拆分产物）
 */
import { invoke } from '@tauri-apps/api/core'

// ==================== Settings Commands ====================

/** 获取所有数据库设置 */
export async function getAllDbSettings(): Promise<Record<string, unknown>> {
  return invoke('get_all_db_settings')
}

/** 设置数据库项 */
export async function setDbSetting(key: string, value: any): Promise<void> {
  return invoke('set_db_setting', { key, value })
}

/** 获取应用设置 */
export async function getAppSettings(): Promise<any> {
  return invoke('get_app_settings')
}

/** 保存应用设置 */
export async function saveAppSettings(settings: any): Promise<void> {
  return invoke('save_app_settings', { settings })
}

// ==================== System Commands ====================

/** Ping 命令，用于测试连接 */
export async function ping(): Promise<string> {
  return invoke('ping')
}

/** 获取应用版本 */
export async function getAppVersion(): Promise<string> {
  return invoke('get_app_version')
}

/** 获取应用启动时间 */
export async function getStartupTime(): Promise<number> {
  return invoke('get_startup_time')
}

/** 获取本地 IP 地址列表 */
export async function getLocalIpAddresses(): Promise<string[]> {
  return invoke('get_local_ip_addresses')
}
