/**
 * Plugin Commands
 *
 * 插件系统 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from './types'

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
  return await invoke<PluginInfo[]>('plugin_list_loaded')
}

/** 获取单个插件信息 */
export async function pluginGetInfo(pluginId: string): Promise<PluginInfo | null> {
  return await invoke<PluginInfo | null>('plugin_get_info', { pluginId })
}

/** 激活插件 */
export async function pluginActivate(pluginId: string): Promise<void> {
  return await invoke('plugin_activate', { pluginId })
}

/** 停用插件 */
export async function pluginDeactivate(pluginId: string): Promise<void> {
  return await invoke('plugin_deactivate', { pluginId })
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
