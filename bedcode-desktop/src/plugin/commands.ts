/**
 * Plugin Commands
 *
 * 插件系统 Tauri invoke 命令封装
 */

import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
import type { PluginInfo } from './types'
import type { AuthStrategy, PluginAuthOverview } from '@/utils/authPolicy'

// ==================== 前端通道身份（审计票 06 / P0-5） ====================

/**
 * 宿主面凭证（loader 会话密钥）缓存
 *
 * 宿主前端 bootstrap 在**导入任何插件模块之前**调用 {@link ensureHostCredential} 取得它，
 * 之后一直复用（页面加载时由宿主 `on_page_load` 钩子重置，本缓存随页面卸载一起消失）。
 * 插件前端拿不到它：密钥「首个调用者生效」，插件代码开始运行时已被宿主占位。
 */
let hostCredentialCache: string | null = null

/**
 * 取得宿主面凭证（幂等）：宿主面命令（`pluginInvoke` / 插件存储 / fs 授权应答）的凭证来源
 *
 * 必须在插件模块导入前调用一次（`pluginLoader.loadAll()` 首行）；之后的宿主调用点复用缓存。
 */
export async function ensureHostCredential(): Promise<string> {
  if (hostCredentialCache) return hostCredentialCache
  hostCredentialCache = await invoke<string>('plugin_frontend_loader_session')
  logger.log('[PluginCmd] host credential acquired (loader session)')
  return hostCredentialCache
}


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

/**
 * 批准用户安装插件的权限清单（ADR 0020 审批门禁）
 *
 * 宿主按当前 manifest 记录批准集 + 目录内容哈希；返回本次批准的权限清单。
 * 批准只解除闸门，不隐式启用（调用方需显式再走 activate）。
 *
 * @returns 批准生效的权限清单（词汇表内的声明位）
 */
export async function pluginApprove(pluginId: string): Promise<string[]> {
  logger.log(`[PluginCmd] pluginApprove(${pluginId}) invoking...`)
  const approved = await invoke<string[]>('plugin_approve', { pluginId })
  logger.log(`[PluginCmd] pluginApprove(${pluginId}) approved ${approved.length} permission(s)`)
  return approved
}

/**
 * 为插件前端签发通道令牌（审计票 06；由宿主 loader 在创建 PluginContext 时调用）
 *
 * 需宿主面凭证 + 插件处于运行态；令牌决定插件面命令的身份（`plugin_id` 只作目标）。
 * 令牌随停用回收，页面加载后需重新签发。
 */
export async function pluginChannelToken(pluginId: string): Promise<string> {
  const loaderSession = await ensureHostCredential()
  const token = await invoke<string>('plugin_channel_token', { pluginId, loaderSession })
  logger.log(`[PluginCmd] pluginChannelToken(${pluginId}) issued`)
  return token
}

/** 标记插件错误 */
export async function pluginMarkError(pluginId: string, error: string): Promise<void> {
  return await invoke('plugin_mark_error', { pluginId, error })
}

/** 从本地 zip 插件包安装（安装到用户插件目录，来源 user-installed）
 *
 * @returns 安装后的 plugin_id
 */
export async function pluginInstallFromFile(path: string): Promise<string> {
  logger.log(`[PluginCmd] pluginInstallFromFile(${path}) invoking...`)
  const id = await invoke<string>('plugin_install_from_file', { path })
  logger.log(`[PluginCmd] pluginInstallFromFile() installed: ${id}`)
  return id
}

/** 卸载插件（所有来源；删除插件所有数据：安装目录 + 存储 + 启用状态） */
export async function pluginUninstall(pluginId: string): Promise<void> {
  logger.log(`[PluginCmd] pluginUninstall(${pluginId}) invoking...`)
  await invoke('plugin_uninstall', { pluginId })
  logger.log(`[PluginCmd] pluginUninstall(${pluginId}) succeeded`)
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

/** 插件存储：获取值（`credential` 决定身份：宿主凭证可读写任意插件，插件令牌只能读写自己） */
export async function pluginStorageGet(
  pluginId: string,
  key: string,
  credential: string,
): Promise<any> {
  return await invoke('plugin_storage_get', { pluginId, key, credential })
}

/** 插件存储：设置值 */
export async function pluginStorageSet(
  pluginId: string,
  key: string,
  value: any,
  credential: string,
): Promise<void> {
  return await invoke('plugin_storage_set', { pluginId, key, value, credential })
}

/** 插件存储：删除值 */
export async function pluginStorageDelete(
  pluginId: string,
  key: string,
  credential: string,
): Promise<void> {
  return await invoke('plugin_storage_delete', { pluginId, key, credential })
}

// 票 08：`pluginTerminalSendInput`（宿主替插件导流终端输入）已注销——
// 插件写自家会话的输入走自有命令通道（`context.commands.execute('session.input', …)`）。

/**
 * 文件授权询问的应答决定（宿主 `fs_auth::FsDecision` 的 wire 值，票 03）
 *
 * - `allow_once`：允许本次，不落账（下次访问同一路径仍会询问）
 * - `allow_remember`：允许并「记住」——**仅「默认」档的弹窗提供**，按本次操作集落
 *   allow 记录；「总是询问」档跳过记录，落一条没人读的记录只会造成两处口径
 * - `deny`：拒绝本次，不落账（下次访问重新询问）
 * - `deny_always`：以后都拒绝（落 deny 记录，该目标与其子树此后被直接拒绝）
 *
 * 不再用 `allowed + remember` 双布尔：它表达不了「以后都拒绝」，四种组合里还有
 * 两种非法态（拒绝 + 记住 / 拒绝 + 不记住 的差别只有宿主知道）。
 */
export type FsAuthDecision = 'allow_once' | 'allow_remember' | 'deny' | 'deny_always'

/**
 * 回复文件系统授权请求（宿主面命令：需宿主凭证）
 *
 * 授权请求事件是广播的，插件前端也能监听到——命令绑定宿主凭证后，
 * 插件无法替用户「同意」自己的文件访问请求（审计票 06）。
 */
export async function pluginFsAuthRespond(
  requestId: string,
  decision: FsAuthDecision,
): Promise<void> {
  const credential = await ensureHostCredential()
  return await invoke('plugin_fs_auth_respond', { requestId, decision, credential })
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

/**
 * 调用 Rust 插件的自定义 command
 *
 * `credential` 决定身份（审计票 06）：宿主面凭证可驱动任意插件的 command（宿主 UI 职权），
 * 插件令牌只能驱动自己的 command。
 */
export async function pluginInvoke(
  pluginId: string,
  command: string,
  args: unknown,
  credential: string,
): Promise<unknown> {
  return await invoke('plugin_invoke', { pluginId, command, args: args ?? null, credential })
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

// ==================== 应用授权（授权策略增强 · 票 01） ====================

/**
 * 应用授权读模型（spec §9.3）：策略 + 授权记录 + 第一方免询问项
 *
 * 宿主面命令（需宿主凭证）：授权记录是安全闸门的配给账，插件前端不得枚举其它
 * 应用的授权情况。`pluginId` 省略 = 总览（全部已安装 wasm 应用）。
 */
export async function pluginAuthOverview(pluginId?: string): Promise<PluginAuthOverview[]> {
  const credential = await ensureHostCredential()
  const apps = await invoke<PluginAuthOverview[]>('plugin_auth_overview', {
    pluginId: pluginId ?? null,
    credential,
  })
  logger.log(`[PluginCmd] pluginAuthOverview(${pluginId ?? 'all'}) returned ${apps.length} app(s)`)
  return apps
}

/**
 * 撤销某目标的授权（spec §8.4：删 allow 记录 + 落一条 `deny` 记录）
 *
 * 宿主面命令（与读模型同判据，宿主后端二次校验）：撤销是安全决策，插件面凭证
 * 不得替用户撤销自己的授权。
 *
 * @returns 被删除的 allow 记录条数（0 = 本就没有 allow 记录）
 */
export async function pluginAuthRevoke(
  pluginId: string,
  resource: string,
  target: string,
): Promise<number> {
  const credential = await ensureHostCredential()
  const removed = await invoke<number>('plugin_auth_revoke', {
    pluginId,
    resource,
    target,
    credential,
  })
  logger.log(
    `[PluginCmd] pluginAuthRevoke(${pluginId}/${resource}) removed ${removed} allow record(s)`,
  )
  return removed
}

/**
 * 移除某目标的 `deny` 记录（spec §8.4 的恢复出口：只删 deny，回到未覆盖状态）
 *
 * @returns 被删除的 deny 记录条数（0 = 本就没有 deny 记录）
 */
export async function pluginAuthRemoveRecord(
  pluginId: string,
  resource: string,
  target: string,
): Promise<number> {
  const credential = await ensureHostCredential()
  const removed = await invoke<number>('plugin_auth_remove_record', {
    pluginId,
    resource,
    target,
    credential,
  })
  logger.log(
    `[PluginCmd] pluginAuthRemoveRecord(${pluginId}/${resource}) removed ${removed} deny record(s)`,
  )
  return removed
}

/**
 * 设置某应用在某资源上的授权策略档位（spec §4.1 三档，票 03 起）
 *
 * 宿主面命令（与读模型 / 撤销同判据）：档位是安全闸门的松紧，插件面凭证不得替
 * 用户改自己的档位。未知档位值由宿主**显性报错**（写面不猜档位：手误写错却静默
 * 存成默认档，用户会以为设置成功了）。
 */
export async function pluginAuthSetStrategy(
  pluginId: string,
  resource: string,
  strategy: AuthStrategy,
): Promise<void> {
  const credential = await ensureHostCredential()
  await invoke('plugin_auth_set_strategy', { pluginId, resource, strategy, credential })
  logger.log(`[PluginCmd] pluginAuthSetStrategy(${pluginId}/${resource}) = ${strategy}`)
}

// ==================== 网络出站授权（授权策略增强 · 票 05） ====================

/**
 * 出站授权询问的三态决定（宿主 `network_auth::NetworkDecision` 的 wire 值）
 *
 * - `allow_once`：允许本次询问覆盖的那批请求，并按 origin 落 allow 记录
 *   （同 origin 后续请求免询问）
 * - `deny`：拒绝本次，不落账（下次访问同一 origin 会再问）
 * - `deny_always`：拒绝本次并落 deny 记录（以后都拒绝）
 *
 * 与 fs 侧的应答形状不共用（fs 侧票 03 起也是枚举，但多一档「记住」）：网络侧
 * 询问粒度就是 origin，没有「只这一次、别记」的中间档（详见
 * `NetworkAuthDialog.vue` 的组件说明）。
 */
export type NetworkAuthDecision = 'allow_once' | 'deny' | 'deny_always'

/**
 * 回复网络出站授权请求（宿主面命令：需宿主凭证）
 *
 * 询问事件是广播的，插件前端也能 `listen` 到——命令绑定宿主凭证后，插件无法替用户
 * 「同意」自己发起的出站访问（与 `pluginFsAuthRespond` 同一威胁模型）。
 */
export async function pluginNetworkAuthRespond(
  requestId: string,
  decision: NetworkAuthDecision,
): Promise<void> {
  const credential = await ensureHostCredential()
  return await invoke('plugin_network_auth_respond', { requestId, decision, credential })
}
