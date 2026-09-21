/**
 * 设置 / 系统命令封装（useDesktopCommands 拆分产物）
 *
 * 收敛记录（2026-09-21）：曾在此封装的 `getAllDbSettings` / `setDbSetting` /
 * `getAppSettings` / `saveAppSettings` / `getStartupTime` / `ping` 实测**零调用方**
 * —— 宿主 `stores/settings.ts` 直接 `invoke('get_app_settings' | 'save_app_settings')`，
 * 其余命令在宿主前端已无消费面 → 全部删除，只保留有消费方的 `getAppVersion`。
 */
import { invoke } from '@tauri-apps/api/core'

// ==================== System Commands ====================

/** 获取应用版本（设置页「关于」分组使用） */
export async function getAppVersion(): Promise<string> {
  return invoke('get_app_version')
}
