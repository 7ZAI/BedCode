/**
 * Mobile Plugin Commands
 *
 * 插件相关 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, PeerFileServiceInfo } from './types'

/** 获取所有已加载插件信息 */
export async function pluginListLoaded(): Promise<PluginInfo[]> {
  return await invoke('plugin_list_loaded')
}

/** 获取单个插件信息 */
export async function pluginGetInfo(pluginId: string): Promise<PluginInfo | null> {
  return await invoke('plugin_get_info', { pluginId })
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

// ==================== File Service ====================

/** 文件服务挂载结果（与 SDK Rust MountResult camelCase 对应） */
export interface FileSrvMountResult {
  mountPath: string
  basePath: string
}

/** 挂载文件服务（TS 通道；options 为不含 onUploadRequest 函数的 MountOptions） */
export async function pluginFilesrvMount(
  pluginId: string,
  options: Record<string, unknown>,
): Promise<FileSrvMountResult> {
  return await invoke<FileSrvMountResult>('plugin_filesrv_mount', {
    pluginId,
    optionsJson: JSON.stringify(options),
  })
}

/** 更新挂载点的允许目录根 */
export async function pluginFilesrvUpdateRoots(
  pluginId: string,
  mountPath: string,
  roots: string[],
): Promise<void> {
  return await invoke('plugin_filesrv_update_roots', {
    pluginId,
    mountPath,
    rootsJson: JSON.stringify(roots),
  })
}

/** 摘除挂载点（对应 TS SDK mount.dispose()） */
export async function pluginFilesrvDispose(pluginId: string, mountPath: string): Promise<void> {
  return await invoke('plugin_filesrv_dispose', { pluginId, mountPath })
}

/** 回填 Webview 上传策略钩子决定 */
export async function pluginFilesrvRespondUploadRequest(
  pluginId: string,
  requestId: string,
  allow: boolean,
  reason?: string,
): Promise<void> {
  return await invoke('plugin_filesrv_respond_upload_request', {
    pluginId,
    requestId,
    allow,
    reason: reason ?? null,
  })
}

/** 获取对端文件服务信息（未公告返回 null） */
export async function pluginFilesrvGetPeer(
  pluginId: string,
  peerId: string,
): Promise<PeerFileServiceInfo | null> {
  return await invoke<PeerFileServiceInfo | null>('plugin_filesrv_get_peer', { pluginId, peerId })
}

/** 系统目录选择对话框（用户取消返回 null） */
export async function pluginPickDirectory(pluginId: string): Promise<string | null> {
  return await invoke<string | null>('plugin_pick_directory', { pluginId })
}

/** 系统文件选择对话框（插件上传本地文件用；用户取消返回 null） */
export async function pluginPickFile(pluginId: string): Promise<string | null> {
  return await invoke<string | null>('plugin_pick_file', { pluginId })
}
