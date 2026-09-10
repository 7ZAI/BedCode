/**
 * 日志设置操作（desktop-logging-overhaul 04）
 *
 * 三个后端命令的封装：
 * - `set_log_level`：运行时热调日志文件级别（不落盘，重启回落到配置值）
 * - `open_log_dir`：打开系统文件管理器定位日志目录
 * - `save_log_settings`：持久化日志配置（替换配置文件的 log 段；format/rotation 等重启生效）
 *
 * 后端实现见 commands/system.rs 与 commands/opener.rs。
 */

import { invoke } from '@/utils/invoke'

/** 保存到配置文件的日志段（与后端 LogConfig 对齐，camelCase 序列化） */
export interface LogSettingsPayload {
  fileLevel: string
  rotation: string
  maxFiles: number
  format: string
  capacityBytes: number
  consoleInRelease: boolean
}

export function useLogSettings() {
  /** 运行时热调日志文件级别（debug/info/warn/error） */
  async function setLogLevel(level: string): Promise<void> {
    await invoke('set_log_level', { level })
  }

  /** 打开系统文件管理器定位日志目录 */
  async function openLogDir(): Promise<void> {
    await invoke('open_log_dir')
  }

  /** 持久化日志配置（替换配置文件 log 段；format/rotation/max_files/capacity_bytes 重启生效） */
  async function saveLogSettings(log: LogSettingsPayload): Promise<void> {
    await invoke('save_log_settings', { log })
  }

  return { setLogLevel, openLogDir, saveLogSettings }
}
