/**
 * Plugin Commands
 *
 * 插件系统 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, PeerFileServiceInfo, UploadHookDecision } from './types'

/** Registry entry types from Rust backend */
export interface CommandEntry {
  plugin_id: string
  command_id: string
  title: string
  icon: string | null
}

export interface ViewEntry {
  plugin_id: string
  view_id: string
  view_type: string
  title: string
  component: string
}

export interface FileHandlerEntry {
  plugin_id: string
  handler_id: string
  extensions: string[]
  viewer: string
  icon: string | null
}

/** 获取所有已加载插件 */
export async function pluginListLoaded(): Promise<PluginInfo[]> {
  console.log('[PluginCmd] pluginListLoaded() invoking...')
  const result = await invoke<PluginInfo[]>('plugin_list_loaded')
  console.log(`[PluginCmd] pluginListLoaded() returned ${result.length} plugin(s)`)
  return result
}

/** 获取单个插件信息 */
export async function pluginGetInfo(pluginId: string): Promise<PluginInfo | null> {
  console.log(`[PluginCmd] pluginGetInfo(${pluginId}) invoking...`)
  const result = await invoke<PluginInfo | null>('plugin_get_info', { pluginId })
  console.log(`[PluginCmd] pluginGetInfo(${pluginId}) returned:`, result ? `state=${result.state.state}` : 'null')
  return result
}

/** 激活插件 */
export async function pluginActivate(pluginId: string): Promise<void> {
  console.log(`[PluginCmd] pluginActivate(${pluginId}) invoking...`)
  await invoke('plugin_activate', { pluginId })
  console.log(`[PluginCmd] pluginActivate(${pluginId}) succeeded`)
}

/** 停用插件 */
export async function pluginDeactivate(pluginId: string): Promise<void> {
  console.log(`[PluginCmd] pluginDeactivate(${pluginId}) invoking...`)
  await invoke('plugin_deactivate', { pluginId })
  console.log(`[PluginCmd] pluginDeactivate(${pluginId}) succeeded`)
}

/** 标记插件错误 */
export async function pluginMarkError(pluginId: string, error: string): Promise<void> {
  return await invoke('plugin_mark_error', { pluginId, error })
}

/** 插件存储：获取值 */
export async function pluginStorageGet(pluginId: string, key: string): Promise<any> {
  return await invoke('plugin_storage_get', { pluginId, key })
}

/** 插件存储：设置值 */
export async function pluginStorageSet(pluginId: string, key: string, value: any): Promise<void> {
  return await invoke('plugin_storage_set', { pluginId, key, value })
}

/** 插件存储：删除值 */
export async function pluginStorageDelete(pluginId: string, key: string): Promise<void> {
  return await invoke('plugin_storage_delete', { pluginId, key })
}

/** 插件终端：发送输入 */
export async function pluginTerminalSendInput(pluginId: string, sessionId: string, text: string): Promise<void> {
  return await invoke('plugin_terminal_send_input', { pluginId, sessionId, text })
}

/** 获取所有命令 */
export async function pluginListCommands(): Promise<CommandEntry[]> {
  return await invoke<CommandEntry[]>('plugin_list_commands')
}

/** 获取指定类型的视图 */
export async function pluginListViews(viewType: string): Promise<ViewEntry[]> {
  return await invoke<ViewEntry[]>('plugin_list_views', { viewType })
}

/** 查找文件处理器 */
export async function pluginFindFileHandler(extension: string): Promise<FileHandlerEntry | null> {
  return await invoke<FileHandlerEntry | null>('plugin_find_file_handler', { extension })
}

/** Rust 插件 command 入口 */
export interface PluginCommandEntry {
  plugin_id: string
  command_name: string
  title: string
}

/** 调用 Rust 插件的自定义 command */
export async function pluginInvoke(pluginId: string, command: string, args?: unknown): Promise<unknown> {
  return await invoke('plugin_invoke', { pluginId, command, args: args ?? null })
}

/** 获取所有 Rust 插件的 command 列表 */
export async function pluginListRustCommands(): Promise<PluginCommandEntry[]> {
  return await invoke<PluginCommandEntry[]>('plugin_list_rust_commands')
}

/** 热重载插件（仅开发模式可用） */
export async function pluginDevReload(pluginId: string): Promise<void> {
  return await invoke('plugin_dev_reload', { pluginId })
}

/** 获取插件激活状态映射（plugin_id → is_activated） */
export async function pluginGetActivatedState(): Promise<Record<string, boolean>> {
  return await invoke<Record<string, boolean>>('plugin_get_activated_state')
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

/** 系统多文件选择对话框（上传方向用；用户取消返回空数组） */
export async function pluginPickFiles(pluginId: string): Promise<string[]> {
  return await invoke<string[]>('plugin_pick_files', { pluginId })
}

// ==================== v2 传输批命令 ====================

/** 批准传输批（接收端应答「接受全部」） */
export async function pluginFilesrvApproveTransfer(
  pluginId: string,
  batchId: string,
): Promise<void> {
  return await invoke('plugin_filesrv_approve_transfer', { pluginId, batchId })
}

/** 拒绝传输批（接收端应答「拒绝全部」） */
export async function pluginFilesrvRejectTransfer(
  pluginId: string,
  batchId: string,
): Promise<void> {
  return await invoke('plugin_filesrv_reject_transfer', { pluginId, batchId })
}

/** 设置批准超时（秒，10–600；仅 ask 策略生效） */
export async function pluginFilesrvSetApprovalTimeout(
  pluginId: string,
  mountPath: string,
  seconds: number,
): Promise<void> {
  return await invoke('plugin_filesrv_set_approval_timeout', { pluginId, mountPath, seconds })
}

/** 取消接收中的上传会话（接收端本地取消，session 级） */
export async function pluginFilesrvCancelReceiving(
  pluginId: string,
  sessionId: string,
): Promise<void> {
  return await invoke('plugin_filesrv_cancel_receiving', { pluginId, sessionId })
}

/** 回填 Webview 批量传输请求钩子决定（v2，decision 为 UploadHookDecision） */
export async function pluginFilesrvRespondTransferRequest(
  pluginId: string,
  requestId: string,
  decision: UploadHookDecision,
): Promise<void> {
  return await invoke('plugin_filesrv_respond_transfer_request', {
    pluginId,
    requestId,
    decisionJson: JSON.stringify(decision),
  })
}

/** 在系统文件管理器中显示文件/目录（需 system:open 权限） */
export async function pluginRevealInDir(pluginId: string, path: string): Promise<void> {
  return await invoke<void>('plugin_reveal_in_dir', { pluginId, path })
}
