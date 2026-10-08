/**
 * 移动端宿主壳（Host Shell）契约
 * -----------------------------------------------------------------------------
 * 本文件是「壳」与「应用形态」之间的唯一契约面。壳内所有 UI、导航、权限展示
 * 只依赖这里的类型，不依赖插件系统、不依赖 WASM 运行时的任何具体形态。
 *
 * 为什么单独抽一层契约：
 *   移动端当前的应用真源是插件系统（src/plugin/**），后续的 WASM 应用平台会把
 *   真源换成 wasm app 清单。两者形态不同但「壳要消费的东西」完全一致——应用清单、
 *   运行态、权限授予、以及应用向壳贡献的界面。把它们收敛成一份契约，切换真源时
 *   只需换一个 ShellAppSource 实现，壳内零改动。
 *
 * 边界纪律（与 AGENTS.md §5.1 同源）：
 *   壳只回答「平台怎么组织应用」，不回答「某个应用做什么业务」。应用内部的业务
 *   由其 surface 组件自持，壳只负责挂载、生命周期与权限闸门。
 */

import type { Component } from 'vue'

/** 可释放句柄：注册扩展点的返回值，dispose 即摘除该注册 */
export interface Disposable {
  dispose(): void
}

/** 应用在壳内的运行态（壳语义，非插件运行时状态） */
export type ShellAppState =
  /** 前台/后台运行中 */
  | 'running'
  /** 已安装但未启动 */
  | 'stopped'
  /** 被用户或平台停用（启用后才可启动） */
  | 'disabled'
  /** 上次启动失败（保留数据，可重试） */
  | 'error'

/** 单条权限的授予情况 */
export interface ShellPermissionGrant {
  /** 权限词，与移动端权限词汇真源一致（如 terminal:output / fs:write / bus） */
  key: string
  /** 是否已授予 */
  granted: boolean
  /**
   * 默认授予且不可关闭（storage 等底座能力）。
   * UI 上置灰并标注「默认授予」，而不是隐藏——隐藏会让用户误以为应用没有该能力。
   */
  locked?: boolean
  /** 授予范围补充说明（如「已授权 3 个目录」「仅限已配对主机」） */
  scope?: string
}

/** 应用贡献给平台首页的快捷卡片 */
export interface ShellSlotContribution {
  id: string
  /** 卡片组件：接收 { app: ShellApp } prop，内容由应用自持 */
  component: Component
  /** 排序权重，小者靠前；缺省 100 */
  order?: number
}

/** 应用的运行面（在壳内被打开时挂载） */
export interface ShellSurfaceContribution {
  /** 运行面组件：接收 { app: ShellApp } prop */
  component: Component
  /**
   * 应用自带主题色。仅作用于应用内部（由其 surface 自行消费），
   * 绝不允许写回平台 token——平台 token 归用户色板，应用 accent 归应用。
   */
  accent?: string
}

/** 应用贡献给「胶囊菜单」的附加项（与小程序胶囊一致：平台叠加在应用之上的控制项） */
export interface ShellCapsuleItem {
  id: string
  label: string
  /** SVG path d（24×24 视框，stroke 风格）；缺省用平台通用图标 */
  icon?: string
  order?: number
  onSelect?: (appId: string) => void | Promise<void>
}

/** 应用贡献给「平台设置」的入口 */
export interface ShellSettingsEntry {
  id: string
  label: string
  /** 右侧摘要文案 */
  hint?: string
  /** SVG path d（24×24 视框） */
  icon?: string
  order?: number
  onSelect?: () => void | Promise<void>
}

/** 应用对宿主壳的全部贡献（可整体随清单下发，也可运行时逐项注册） */
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
  /** 图标：emoji 或 SVG path d；两者都不是时宿主生成首字母回退 */
  icon?: string
  /** 官方应用标记（用于「官方」徽标） */
  official?: boolean
  state: ShellAppState
  permissions: ShellPermissionGrant[]
  /** 应用数据占用（字节）；数据源未统计时为 undefined，UI 显示「—」而非伪造 0 */
  sizeBytes?: number
  /** 上一次失败的简短原因（state === 'error' 时展示） */
  error?: string
  contributions: ShellAppContributions
}

/**
 * 应用数据源：宿主壳与具体应用形态之间的唯一耦合面。
 *
 * 新增形态（插件 / WASM 应用 / 内置能力）只需实现本接口并注册到 ShellRegistry，
 * 壳内 UI 无需任何改动。能力是可选的：不支持的能力返回 false / undefined，
 * 宿主据此置灰并说明原因，不做静默降级。
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
  /**
   * 逐项变更权限授予。
   * @returns true 已生效；false 数据源不支持该能力（宿主 UI 据此置灰并提示）
   */
  setPermissionGrant?(
    appId: string,
    permissionKey: string,
    granted: boolean,
  ): Promise<boolean>
  /**
   * 解析应用运行面组件（数据源延迟提供 surface 时用，如从插件注册表取工具箱视图）。
   * 未实现时宿主回退到应用自带的 contributions.surface。
   */
  resolveSurface?(appId: string): Component | undefined
  /**
   * 安装本地应用包（数据源自行完成选包与校验）。
   * 未实现时宿主不渲染「安装本地包」入口——不提供点了没反应的按钮。
   */
  installFromLocalPackage?(): Promise<boolean>
  /**
   * 卸载应用并删除其数据。
   * 未实现时宿主不渲染「卸载」入口，同理不留死按钮。
   */
  remove?(appId: string): Promise<boolean>
}
