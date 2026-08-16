/**
 * Mobile Plugin Commands
 *
 * 插件相关 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import type {
  PluginInfo,
  PeerFileServiceInfo,
  SafEntry,
  SafCopyHandle,
  SafCopyStatus,
  PickedSharedDirectory,
} from './types'

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

// ==================== v2 批量传输批准（TS 通道） ====================

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

/** 设置批准超时（秒，10–600） */
export async function pluginFilesrvSetApprovalTimeout(
  pluginId: string,
  mountPath: string,
  seconds: number,
): Promise<void> {
  return await invoke('plugin_filesrv_set_approval_timeout', { pluginId, mountPath, seconds })
}

/** 取消接收中的上传会话（本地取消） */
export async function pluginFilesrvCancelReceiving(
  pluginId: string,
  sessionId: string,
): Promise<void> {
  return await invoke('plugin_filesrv_cancel_receiving', { pluginId, sessionId })
}

/** 回填 Webview 批量传输钩子决定（decision 为 UploadHookDecision JSON） */
export async function pluginFilesrvRespondTransferRequest(
  pluginId: string,
  requestId: string,
  decisionJson: string,
): Promise<void> {
  return await invoke('plugin_filesrv_respond_transfer_request', {
    pluginId,
    requestId,
    decisionJson,
  })
}

/** 系统目录选择对话框（用户取消返回 null） */
export async function pluginPickDirectory(pluginId: string): Promise<string | null> {
  return await invoke<string | null>('plugin_pick_directory', { pluginId })
}

/** 系统文件选择对话框（插件上传本地文件用；用户取消返回 null） */
export async function pluginPickFile(pluginId: string): Promise<string | null> {
  return await invoke<string | null>('plugin_pick_file', { pluginId })
}

/**
 * 查询/引导「所有文件访问权限」（Android 11+ 分区存储）
 *
 * 未授权时宿主跳转系统授权页；返回跳转前是否已授权。非 Android 平台 reject。
 */
export async function pluginOpenAllFilesSettings(pluginId: string): Promise<boolean> {
  return await invoke<boolean>('open_all_files_settings', { pluginId })
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

// ==================== SAF 存储访问（SafIo 主 seam） ====================

/** SAF：列出目录树子条目（共享目录 App 内遍历） */
export async function pluginSafListTree(
  pluginId: string,
  treeUri: string,
  documentId: string,
): Promise<SafEntry[]> {
  return await invoke<SafEntry[]>('plugin_saf_list_tree', { pluginId, treeUri, documentId })
}

/** SAF：启动中转复制（SAF 源 → app 私有 cache），返回 {copyId, destPath} */
export async function pluginSafCopyStart(
  pluginId: string,
  uri: string,
  destName: string,
): Promise<SafCopyHandle> {
  return await invoke<SafCopyHandle>('plugin_saf_copy_start', { pluginId, uri, destName })
}

/** SAF：轮询中转复制进度 */
export async function pluginSafCopyStatus(
  pluginId: string,
  copyId: string,
): Promise<SafCopyStatus> {
  return await invoke<SafCopyStatus>('plugin_saf_copy_status', { pluginId, copyId })
}

/** SAF：取消中转复制 */
export async function pluginSafCopyCancel(pluginId: string, copyId: string): Promise<void> {
  return await invoke<void>('plugin_saf_copy_cancel', { pluginId, copyId })
}

/** SAF：清扫中转复制残留（file-transfer 插件激活时调用） */
export async function pluginSafCleanupStaleCopies(pluginId: string): Promise<void> {
  return await invoke<void>('plugin_saf_cleanup_stale_copies', { pluginId })
}

/** SAF：检测树授权是否仍有效 */
export async function pluginSafCheckAuthorized(
  pluginId: string,
  treeUri: string,
): Promise<boolean> {
  return await invoke<boolean>('plugin_saf_check_authorized', { pluginId, treeUri })
}

/** 弹系统目录树选择器，返回 SAF 树元数据（添加共享目录条目用；取消返回 null） */
export async function pluginPickSharedDirectory(
  pluginId: string,
): Promise<PickedSharedDirectory | null> {
  const picked = await invoke<[string, string, string] | null>('plugin_pick_shared_directory', {
    pluginId,
  })
  if (!picked) return null
  return { uri: picked[0], documentId: picked[1], displayName: picked[2] }
}

/** 列出真实路径目录条目（免授权特殊条目「app 私有下载目录」浏览用） */
export async function pluginSafListDir(pluginId: string, path: string): Promise<SafEntry[]> {
  return await invoke<SafEntry[]>('plugin_saf_list_dir', { pluginId, path })
}
