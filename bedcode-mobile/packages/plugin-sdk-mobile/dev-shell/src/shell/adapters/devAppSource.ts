/**
 * dev-shell 数据源：调试对象 → 壳内应用
 * -----------------------------------------------------------------------------
 * 这是「壳」与「被调试对象」之间的唯一耦合面，与宿主 `src/shell/adapters/pluginAppSource.ts`
 * 同位置同职责：宿主把插件运行时投影成 ShellApp，本文件把 dev-shell 的调试记录
 * （内置应用 + 被调试插件）投影成 ShellApp。壳内 UI 对两者无感。
 *
 * 两处与宿主不同，都是浏览器环境的必然结果，不是偷懒：
 *   1. 启动 = `loader.activatePlugin`（重新跑插件 activate），停止 = `deactivatePlugin`；
 *      没有 WASM 后端，也就没有宿主那套 runtime 生命周期。
 *   2. 权限授予是纯展示：dev-shell 全程跳过权限检查（README「Mock 边界」已声明），
 *      因此 setPermissionGrant 明确返回 false，壳据此把开关置灰并说明原因——
 *      不做「本地写一下就假装生效」的降级。
 */

import { plugins, type DevPluginRecord } from '../../registry'
import { isLockedPermission } from '../permissions'
import { getShellRegistry } from '../registry'
import type {
  ShellApp,
  ShellAppSource,
  ShellAppState,
  ShellPermissionGrant,
} from '../types'

/** 数据源标识 */
export const DEV_APP_SOURCE_ID = 'dev-shell'

/** 无可用图标时的通用图形（24×24 path d，与壳内 path 图标同风格） */
const FALLBACK_ICON_PATH = 'M9.5 3a6.5 6.5 0 100 13 6.5 6.5 0 000-13z M14.5 12.5l6 6'

/**
 * manifest 里的 icon 是**文件名**（如 `icon.svg`），不是 emoji 也不是 path d。
 *
 * 壳的图标契约只有 emoji / path d 两态，直接透传会被判成 emoji 并把 "icon.svg"
 * 当字面量渲染出来。预览环境读不到包内资源（插件未打包），因此统一回退到通用图形；
 * 真机上由宿主数据源解析真实图标，这条只影响 dev-shell。
 */
function resolveIcon(icon: unknown): string | undefined {
  if (typeof icon !== 'string' || !icon) return undefined
  if (/\.(svg|png|webp|jpe?g)$/i.test(icon)) return FALLBACK_ICON_PATH
  return icon
}

/** 调试记录状态 → 壳内运行态（口径对齐宿主 adapter 的映射表） */
function mapState(record: DevPluginRecord): ShellAppState {
  switch (record.state) {
    case 'activated':
      return 'running'
    case 'error':
      return 'error'
    case 'deactivated':
      return 'disabled'
    default:
      return 'stopped'
  }
}

/** 权限清单 → 授予项 */
function mapPermissions(record: DevPluginRecord): ShellPermissionGrant[] {
  const manifest = record.manifest as Record<string, unknown>
  const declared = Array.isArray(manifest.permissions) ? manifest.permissions : []
  const errored = record.state === 'error'
  return declared
    .filter((key): key is string => typeof key === 'string')
    .map((key) => ({
      key,
      // dev-shell 跳过权限检查（全部视为授予）；加载失败的应用显示为未授予，
      // 免得「明明没起来」却在列表里显示成满权限在跑
      granted: !errored,
      locked: isLockedPermission(key),
    }))
}

/** 单条调试记录 → 壳内应用模型 */
export function toShellApp(record: DevPluginRecord): ShellApp {
  const manifest = record.manifest as Record<string, unknown>
  const str = (key: string): string | undefined =>
    typeof manifest[key] === 'string' ? (manifest[key] as string) : undefined

  return {
    id: record.id,
    name: record.name,
    version: str('version') ?? '0.0.0',
    description: str('description'),
    author: str('author'),
    icon: resolveIcon(manifest.icon),
    official: record.builtin === true,
    state: mapState(record),
    // 失败原因随状态一起投影：应用列表与详情页都靠它说明「为什么启动失败」，
    // 只给一个「启动失败」徽标等于让用户自己猜
    error: record.error,
    permissions: mapPermissions(record),
    // 运行面不从清单下发：插件在 activate 时才注册 surface，因此走 resolveSurface
    contributions: {},
  }
}

/**
 * 数据源对「怎么启动/停止一个调试对象」不做假设，只接受注入
 *
 * 依赖倒置的好处：适配器因此可以在没有加载器（及其 SFC 依赖链）的环境里被测，
 * 且换启动策略不必改壳内任何代码。
 */
export interface DevAppSourceDeps {
  activate: (appId: string) => Promise<void>
  deactivate: (appId: string) => Promise<void>
}

/**
 * dev-shell 数据源
 *
 * 清单 = 内置应用 + 被调试插件，两者同构并列：内置的「模拟终端」不再是壳的调试页，
 * 而是一个真正的应用（自持运行面），这样它与被调试插件走的是同一条加载与渲染路径。
 */
export function createDevAppSource(deps: DevAppSourceDeps): ShellAppSource {
  return {
    id: DEV_APP_SOURCE_ID,

    async list(): Promise<ShellApp[]> {
      return plugins.value.map(toShellApp)
    },

    async launch(appId: string): Promise<void> {
      await deps.activate(appId)
    },

    async stop(appId: string): Promise<void> {
      await deps.deactivate(appId)
    },

    /**
     * 逐项权限开关：dev-shell 全程跳过权限检查，没有可写的真源
     *
     * 明确返回 false，让壳把开关置灰并说明——「点了就通过」的假成功会让插件开发者
     * 误以为权限逻辑已验证。
     */
    async setPermissionGrant(): Promise<boolean> {
      return false
    },

    /**
     * 延迟解析运行面
     *
     * 与宿主同口径：应用未注册 surface 就返回 undefined，运行屏渲染「该应用尚未提供
     * 运行面」空态并写明原因，绝不静默找替代面。
     *
     * 这里刻意不记日志：本方法被运行屏的 computed 响应式反复调用，一次「没有运行面」
     * 会刷出成百上千条重复日志，把真正有用的 warn 淹掉。空态本身已经把原因写清楚。
     */
    resolveSurface(appId: string) {
      return getShellRegistry().getApp(appId)?.contributions.surface?.component
    },
  }
}