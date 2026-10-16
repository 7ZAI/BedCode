/**
 * Dev Shell 宿主壳契约
 * -----------------------------------------------------------------------------
 * 本文件是 dev-shell 内置 mini 壳与「应用形态」之间的唯一契约面，字段与宿主
 * `bedcode-mobile/src/shell/types.ts` **逐字段一致**（同构要求：插件在 dev-shell
 * 里跑通就必须能在真机壳里跑通，两边形态不同就没有调试价值）。
 *
 * 贡献描述符（surface / slot / capsuleItem / settingsEntry）不在这里重定义，
 * 直接复用 SDK 的 `src/types.ts`——那是插件侧契约的真源，宿主壳与 dev-shell
 * 都只是它的消费方。重定义一份只会制造第三处可能漂移的地方。
 *
 * 漂移防线：`__tests__/shell/contractDrift.test.ts` 比对本文件与宿主 types.ts
 * 的字段集，宿主新增字段而这里没跟随时直接测红。
 *
 * 边界纪律（与 AGENTS.md §5.1 同源）：壳只回答「平台怎么组织应用」，不回答
 * 「某个应用做什么业务」。应用内部的业务由其 surface 组件自持。
 */

import type { Component } from 'vue'
import type {
  ShellCapsuleItem,
  ShellSettingsEntry,
  ShellSlotContribution,
  ShellSurfaceContribution,
} from '../../../src/types'

export type {
  ShellCapsuleItem,
  ShellSettingsEntry,
  ShellSlotContribution,
  ShellSurfaceContribution,
}

/** 可释放句柄：注册扩展点的返回值，dispose 即摘除该注册 */
export interface Disposable {
  dispose(): void
}

/** 应用在壳内的运行态（壳语义，与宿主 union 逐字一致） */
export type ShellAppState =
  /** 前台/后台运行中 */
  | 'running'
  /** 已安装但未启动 */
  | 'stopped'
  /** 待人工审批（权限整单批准 + 内容哈希钉扎）：批准前不可启动 */
  | 'approval'
  /** 被用户或平台停用（启用后才可启动） */
  | 'disabled'
  /** 上次启动失败（保留数据，可重试） */
  | 'error'

/** 单条权限的授予情况 */
export interface ShellPermissionGrant {
  /** 权限词（与移动端权限词汇真源一致，如 terminal:output / fs:write / bus） */
  key: string
  /** 是否已授予 */
  granted: boolean
  /** 默认授予且不可关闭（storage 等底座能力）：置灰并标注，不隐藏 */
  locked?: boolean
  /** 授予范围补充说明 */
  scope?: string
}

/** 应用对宿主壳的全部贡献（可随清单下发，也可运行时逐项注册） */
export interface ShellAppContributions {
  slots?: ShellSlotContribution[]
  surface?: ShellSurfaceContribution
  capsuleItems?: ShellCapsuleItem[]
  settingsEntries?: ShellSettingsEntry[]
}

/** 宿主壳中的应用模型 */
export interface ShellApp {
  id: string
  name: string
  version: string
  description?: string
  author?: string
  /** 图标：emoji / SVG path d / 相对插件目录的图片路径（如 icon.svg）；都取不到时壳生成首字母回退 */
  icon?: string
  /**
   * 图片图标的已解析地址（asset protocol URL）：数据源能解析时必须给出，否则首字母回退。
   * dev-shell 的浏览器预览读不到插件包内资源（adapters/devAppSource.ts 的 resolveIcon 会把
   * 图片路径换成通用 path 图形），因此调试数据源当前不产出该字段；保留字段是为与宿主
   * ShellApp 契约逐字一致（dev-shell/__tests__/shell/contractDrift.test.ts 钉住）。
   */
  iconUrl?: string
  /** 官方应用标记 */
  official?: boolean
  state: ShellAppState
  permissions: ShellPermissionGrant[]
  /** 应用数据占用（字节）；未统计时 undefined，UI 显示「—」而非伪造 0 */
  sizeBytes?: number
  /** 上一次失败的简短原因（state === 'error' 时展示） */
  error?: string
  contributions: ShellAppContributions
}

/**
 * 应用数据源：壳与具体应用形态之间的唯一耦合面
 *
 * 能力可选：不支持的能力不实现该方法，壳据此不渲染对应入口，不做静默降级。
 */
export interface ShellAppSource {
  /** 数据源标识（日志与诊断用） */
  readonly id: string
  /** 拉取应用清单（全量覆盖该数据源此前的记录） */
  list(): Promise<ShellApp[]>
  /** 启动应用（幂等：已运行直接返回） */
  launch(appId: string): Promise<void>
  /** 停止应用（幂等：未运行直接返回） */
  stop(appId: string): Promise<void>
  /** 逐项变更权限授予；@returns true 已生效，false 不支持（壳据此置灰并说明） */
  setPermissionGrant?(
    appId: string,
    permissionKey: string,
    granted: boolean,
  ): Promise<boolean>
  /**
   * 批准应用的权限申请（整单：批准全部声明权限 + 内容哈希钉扎）。
   * 未实现时壳不渲染「批准」入口——不提供点了没反应的按钮。
   */
  approve?(appId: string): Promise<void>
  /** 延迟解析运行面组件；未实现时壳回退到应用自带的 contributions.surface */
  resolveSurface?(appId: string): Component | undefined
  /**
   * 安装本地应用包（数据源自行完成选包与校验）。
   *
   * @returns 安装后的应用 id；`null` = 用户取消选包（不是失败）。
   *          未实现时壳不渲染「安装本地包」入口——不提供点了没反应的按钮。
   */
  installFromLocalPackage?(): Promise<string | null>
  /** 卸载应用并删除其数据；未实现时壳不渲染「卸载」入口 */
  remove?(appId: string): Promise<boolean>
}