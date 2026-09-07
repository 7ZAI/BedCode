/**
 * Mobile Plugin Commands
 *
 * 插件相关 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from './types'

/** 获取所有已加载插件信息 */
export async function pluginListLoaded(): Promise<PluginInfo[]> {
  return await invoke('plugin_list_loaded')
}

/** 获取单个插件信息 */
export async function pluginGetInfo(pluginId: string): Promise<PluginInfo | null> {
  return await invoke('plugin_get_info', { pluginId })
}

/**
 * 预授权（启用前置，独立于 activate 供前端先行调用）
 *
 * 时序契约：toggle 启用时先调本命令（此阶段不显示 LoadingDialog，授权弹窗
 * 可正常交互）→ 通过后再显示 loading 并调 pluginActivate；拒绝则直接失败
 */
export async function pluginPreauthorize(pluginId: string): Promise<void> {
  return await invoke('plugin_preauthorize', { pluginId })
}

/** 激活插件 */
export async function pluginActivate(pluginId: string): Promise<void> {
  return await invoke('plugin_activate', { pluginId })
}

/** 停用插件 */
export async function pluginDeactivate(pluginId: string): Promise<void> {
  return await invoke('plugin_deactivate', { pluginId })
}

/** 查询插件启用状态 */
export async function pluginIsEnabled(pluginId: string): Promise<boolean> {
  return await invoke('plugin_is_enabled', { pluginId })
}

/** 设置插件启用状态 */
export async function pluginSetEnabled(pluginId: string, enabled: boolean): Promise<void> {
  return await invoke('plugin_set_enabled', { pluginId, enabled })
}

/** 标记插件错误 */
export async function pluginMarkError(pluginId: string, error: string): Promise<void> {
  return await invoke('plugin_mark_error', { pluginId, error })
}

/** 插件显式上报启动成功（Error → Activated 自愈） */
export async function pluginReportReady(pluginId: string): Promise<void> {
  return await invoke('plugin_report_ready', { pluginId })
}

/** 批准插件权限（人工审批：记录权限清单 + 目录内容哈希钉扎） */
export async function pluginApprove(pluginId: string): Promise<void> {
  return await invoke('plugin_approve', { pluginId })
}

/** 获取插件存储值 */
export async function pluginStorageGet(pluginId: string, key: string): Promise<any> {
  return await invoke('plugin_storage_get', { pluginId, key })
}

/** 设置插件存储值 */
export async function pluginStorageSet(pluginId: string, key: string, value: any): Promise<void> {
  return await invoke('plugin_storage_set', { pluginId, key, value })
}

/** 删除插件存储值 */
export async function pluginStorageDelete(pluginId: string, key: string): Promise<void> {
  return await invoke('plugin_storage_delete', { pluginId, key })
}

/** 插件日志输出 */
export async function pluginLog(pluginId: string, level: string, message: string): Promise<void> {
  return await invoke('plugin_log', { pluginId, level, message })
}

/** 调用 WASM 插件命令 */
export async function pluginInvoke(pluginId: string, command: string, args: any = null): Promise<any> {
  return await invoke('plugin_invoke', { pluginId, command, args })
}

/** 从本地 zip 插件包安装 */
export async function pluginInstallFromFile(path: string): Promise<string> {
  return await invoke('plugin_install_from_file', { path })
}

/** 从 URL 下载 zip 插件包安装 */
export async function pluginDownload(zipUrl: string): Promise<string> {
  return await invoke('plugin_download', { zipUrl })
}

/** 卸载插件（仅用户安装的插件） */
export async function pluginUninstall(pluginId: string): Promise<void> {
  return await invoke('plugin_uninstall', { pluginId })
}

/** 用系统查看器打开已下载文件（需 system:open 权限） */
export async function pluginOpenFile(
  pluginId: string,
  path: string,
  displayName: string,
): Promise<void> {
  return await invoke<void>('plugin_open_file', { pluginId, path, displayName })
}

/** 用系统文件管理器打开文件所在目录（历史记录「打开所在文件夹」；需 system:open 权限） */
export async function pluginOpenFileLocation(pluginId: string, path: string): Promise<void> {
  return await invoke<void>('plugin_open_file_location', { pluginId, path })
}

/** 按文件名打开接收文件所在目录（历史「打开所在文件夹」真机路径；需 system:open 权限） */
export async function pluginRevealReceivedFile(pluginId: string, fileName: string): Promise<void> {
  return await invoke<void>('plugin_reveal_received_file', { pluginId, fileName })
}
