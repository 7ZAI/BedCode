/**
 * 插件系统 → 宿主壳 的数据源适配
 * -----------------------------------------------------------------------------
 * 这是宿主壳与现有插件系统之间的**唯一**耦合点。壳内 UI、导航、权限展示都不认识
 * 插件；反过来，插件系统也不需要知道壳的存在。后续真源换成 WASM 应用平台时，
 * 只需新增一个实现 ShellAppSource 的适配器并替换注册处，壳内零改动。
 *
 * 能力诚实原则：后端目前没有「逐项开关权限」的命令，因此 setPermissionGrant
 * 明确返回 false（宿主据此置灰并说明原因），而不是假装开关生效。
 */

import { open } from '@tauri-apps/plugin-dialog'
import { logger } from '@/utils/frontendLogger'
import {
  pluginActivate,
  pluginDeactivate,
  pluginGetInfo,
  pluginListLoaded,
  pluginInstallFromFile,
  pluginPreauthorize,
  pluginUninstall,
} from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import { getShellRegistry } from '@/shell/registry'
import type { PluginInfo } from '@/plugin/types'
import type { ShellApp, ShellAppSource, ShellAppState, ShellPermissionGrant } from '../types'
import { isLockedPermission } from '../permissions'

/** 数据源标识（与插件系统解耦后改名不影响调用方） */
export const PLUGIN_APP_SOURCE_ID = 'plugin-system'

/** 插件内置来源标记：内置插件视为官方应用 */
function isOfficial(info: PluginInfo): boolean {
  return info.source === 'apk-asset'
}

/**
 * 插件运行时状态 → 壳内运行态
 *
 * Degraded 归为 running：后端实例仍在、命令可用，只是带降级原因——把它显示成
 * 「已停止」会让用户误以为应用没起来。降级原因经 warn 落日志，不占用 UI 主信息位。
 */
function mapState(info: PluginInfo): ShellAppState {
  switch (info.state.state) {
    case 'Activated':
      return 'running'
    case 'Degraded':
      logger.warn(`[ShellAdapter] plugin degraded but alive: id=${info.id}`)
      return 'running'
    case 'NeedsApproval':
      return 'disabled'
    case 'Error':
      return 'error'
    case 'Activating':
      // 激活中：后端尚未给出终态，按未启动呈现，刷新后自愈
      return 'stopped'
    case 'Deactivated':
      return 'disabled'
    case 'Loaded':
    default:
      return 'stopped'
  }
}

/**
 * 权限清单 → 授予项
 *
 * granted 的口径是「清单声明且已通过审批」：NeedsApproval 表示尚未人工批准，
 * 其余状态表示审批已完成。这里不编造逐项授予状态——后端没有暴露它。
 */
function mapPermissions(info: PluginInfo): ShellPermissionGrant[] {
  const approved = info.state.state !== 'NeedsApproval'
  return info.permissions.map((key) => ({
    key,
    granted: approved,
    locked: isLockedPermission(key),
  }))
}

/** 单条插件信息 → 壳内应用模型 */
export function toShellApp(info: PluginInfo): ShellApp {
  return {
    id: info.id,
    name: info.name,
    version: info.version,
    description: info.description,
    author: info.author,
    icon: info.icon,
    official: isOfficial(info),
    state: mapState(info),
    permissions: mapPermissions(info),
    sizeBytes: typeof info.sizeBytes === 'number' ? info.sizeBytes : undefined,
    error: info.state.state === 'Error' ? info.state.error : undefined,
    // 运行面不从清单下发：插件的 UI 入口在激活后才注册进插件注册表，
    // 因此在 resolveSurface 里按需解析（见下）
    contributions: {},
  }
}

/**
 * 插件系统数据源
 *
 * launch 走 pluginLoader.activate（后端激活 + 前端模块加载 + 扩展点注册），
 * 保证从壳里启动的应用与从既有插件页启用的一致；rust-only 插件无前端模块，
 * 直接调后端激活，避免 loader 尝试 import 不存在的前端入口。
 */
export function createPluginAppSource(): ShellAppSource {
  return {
    id: PLUGIN_APP_SOURCE_ID,

    async list(): Promise<ShellApp[]> {
      const infos = await pluginListLoaded()
      return infos.map(toShellApp)
    },

    async launch(appId: string): Promise<void> {
      // 预授权独立前置：未批准的插件在此弹出审批（授权弹窗需可交互，
      // 不能压在 loading 之下），用户拒绝即抛出、启动失败
      await pluginPreauthorize(appId)

      // rust-only 插件没有前端模块入口，loader 会尝试 import 不存在的入口而失败，
      // 因此这类插件只做后端激活
      const info = await pluginGetInfo(appId)
      if (info?.pluginType === 'rust') {
        await pluginActivate(appId)
        return
      }
      await pluginLoader.activate(appId)
    },

    async stop(appId: string): Promise<void> {
      // pluginLoader.deactivate 是幂等的对称拆解：前端注册表清理 + 后端停用
      await pluginLoader.deactivate(appId)
      // 兜底：前端从未加载过的插件（如 rust-only）loader 无记录，仍需通知后端
      await pluginDeactivate(appId)
    },

    /**
     * 安装本地应用包：复用插件包安装链路（zip），需经审批与哈希钉扎
     *
     * 用户取消选包返回 false，不算失败——调用方据此不弹错误提示。
     */
    async installFromLocalPackage(): Promise<boolean> {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'WasmApp', extensions: ['zip'] }],
      })
      if (typeof selected !== 'string') return false
      await pluginInstallFromFile(selected)
      return true
    },

    /** 卸载应用并删除数据（仅用户安装的包可卸载，内置插件由后端拒绝并抛错） */
    async remove(appId: string): Promise<boolean> {
      await pluginUninstall(appId)
      return true
    },

    /**
     * 逐项权限开关：后端暂无对应命令，明确返回 false
     *
     * 不在这里做「写入本地并假装生效」的降级——权限是安全面，假成功比不支持更危险。
     */
    async setPermissionGrant(): Promise<boolean> {
      logger.warn(
        '[ShellAdapter] per-permission grant not supported by backend; permission toggles stay read-only',
      )
      return false
    },

    /**
     * 延迟解析运行面：**只认** `ui.registerSurface` 一个扩展点。
     *
     * 票 2026-10-10 批次 C1：这里原有 terminalView → toolbox → navTab → route
     * 四级回退链，是「壳用旧宿主的插件形式加载页面」的实际代码。现已退役——
     * 应用在壳内的运行面只有一个形态：由应用自持的 `registerSurface`。
     *
     * 为什么必须删干净而不是降级为「优先 surface」：
     * - 回退链让应用可以只注册一个内嵌片段就被壳当成整个应用的主面，
     *   壳因此要认识 toolbox/navTab/terminalView/route 四种插件形态——正是本票要拆的耦合；
     * - 且这四级各自带独立权限位，回退即等于「拿到最弱权限也能进主面」。
     * 现改为 fail-visible：应用未注册 surface 就返回 undefined，运行屏渲染
     * 「该应用尚未提供运行面」空态并写明原因，不静默找替代面（§5.1.3 形态 ①）。
     *
     * `registerSurface` 自身不带 requirePermission（context.ts 已注明：运行面不触发
     * 任何宿主能力），因此本方法不再做权限预检——预检只属于已退役的旧扩展点。
     */
    resolveSurface(appId: string) {
      // 运行面已由 context.ui.registerSurface 写进 ShellRegistry（含 PluginViewHost
      // 包装），壳的 useShellApps.resolveSurface 会先读那份。此处是插件数据源的兜底：
      // 仅当应用确实注册过 surface 才补解析，否则交回空态由运行屏显式提示。
      const surface = getShellRegistry().getApp(appId)?.contributions.surface
      if (!surface) {
        logger.warn(
          `[ShellAdapter] app has no surface registered: appId=${appId}（退役扩展点不再回退，运行屏显示空态）`,
        )
        return undefined
      }
      return surface.component
    },
  }
}
