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

/** 在系统默认浏览器中打开外部 URL（http/https 白名单由宿主闸门校验）
 *
 * 迁移原因（前端零资源访问红线，AGENTS.md §6）：原先前端直接调
 * `@tauri-apps/plugin-shell` 的 `open()`，属前端发起 OS 级访问。该能力已随
 * `shell:allow-open` 权限从 `capabilities/default.json` 撤除，改为经宿主命令
 * `open_external_url`（scheme 白名单 fail-closed，见 `system::opener::validate_external_url`）。
 */
export async function openExternalUrl(url: string): Promise<void> {
  return invoke('open_external_url', { url })
}
