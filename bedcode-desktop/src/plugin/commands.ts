/**
 * Plugin Commands
 *
 * 插件系统 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
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
  logger.log('[PluginCmd] pluginListLoaded() invoking...')
  const result = await invoke<PluginInfo[]>('plugin_list_loaded')
  logger.log(`[PluginCmd] pluginListLoaded() returned ${result.length} plugin(s)`)
  return result
}

/** 获取单个插件信息 */
export async function pluginGetInfo(pluginId: string): Promise<PluginInfo | null> {
  logger.log(`[PluginCmd] pluginGetInfo(${pluginId}) invoking...`)
  const result = await invoke<PluginInfo | null>('plugin_get_info', { pluginId })
  logger.log(
    `[PluginCmd] pluginGetInfo(${pluginId}) returned:`,
    result ? `state=${result.state.state}` : 'null',
  )
  return result
}

/**
 * 预授权（启用前置，独立于 activate 供前端先行调用）
 *
 * 时序契约：toggle 启用时先调本命令（此阶段不显示 loading 遮罩，授权弹窗
 * 可正常交互）→ 通过后再显示遮罩并调 pluginActivate；拒绝则直接失败
 */
export async function pluginPreauthorize(pluginId: string): Promise<void> {
  logger.log(`[PluginCmd] pluginPreauthorize(${pluginId}) invoking...`)
  await invoke('plugin_preauthorize', { pluginId })
  logger.log(`[PluginCmd] pluginPreauthorize(${pluginId}) succeeded`)
}

/** 激活插件 */
export async function pluginActivate(pluginId: string): Promise<void> {
  logger.log(`[PluginCmd] pluginActivate(${pluginId}) invoking...`)
  await invoke('plugin_activate', { pluginId })
  logger.log(`[PluginCmd] pluginActivate(${pluginId}) succeeded`)
}

/** 停用插件 */
export async function pluginDeactivate(pluginId: string): Promise<void> {
  logger.log(`[PluginCmd] pluginDeactivate(${pluginId}) invoking...`)
  await invoke('plugin_deactivate', { pluginId })
  logger.log(`[PluginCmd] pluginDeactivate(${pluginId}) succeeded`)
}

/** 标记插件错误 */
export async function pluginMarkError(pluginId: string, error: string): Promise<void> {
  return await invoke('plugin_mark_error', { pluginId, error })
}

/** 上报前端模块加载诊断（宿主内部诊断通道，仅写 tracing 不改状态，spec §3.7 / issue 04）
 *
 * @param stage 失败/成功发生的步骤：import（动态导入）或 activate（前端 activate()）
 */
export async function pluginFrontendLoadReport(
  pluginId: string,
  stage: 'import' | 'activate',
  ok: boolean,
  detail?: string,
): Promise<void> {
  return await invoke('plugin_frontend_load_report', {
    pluginId,
    stage,
    ok,
    detail: detail ?? null,
  })
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
export async function pluginTerminalSendInput(
  pluginId: string,
  sessionId: string,
  text: string,
): Promise<void> {
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
export async function pluginInvoke(
  pluginId: string,
  command: string,
  args?: unknown,
): Promise<unknown> {
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

/** 在系统文件管理器中显示文件/目录（需 system:open 权限） */
export async function pluginRevealInDir(pluginId: string, path: string): Promise<void> {
  return await invoke<void>('plugin_reveal_in_dir', { pluginId, path })
}
