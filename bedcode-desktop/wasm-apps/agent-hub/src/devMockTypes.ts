/**
 * Agent Hub 插件 dev-shell 领域种子类型（插件自有，SDK 不收录）
 *
 * devMock.ts 与 dev-shell mock 消费时按这些形状 cast；子域键（detection /
 * install / skills / providers / usage）与 guest 五个领域模块一一对应，
 * dev-shell 按「子域存在与否」决定是否注入对应命令 mock。
 *
 * 领域状态形状复用 ./types.ts 的 wire 类型（与 guest emit 载荷同构）；
 * 本文件只声明 devMock 容器与插件自有扩展字段（安装剧本 / skill 内容 /
 * 导入发现 / 应用文件清单——均为演示数据，guest 真实实现从磁盘/进程采集）。
 */
import type {
  InstallDomainState,
  ProvidersDomainState,
  AgentHubState,
  SkillsDomainState,
  UsageSessionDetail,
  UsageSessionRow,
  UsageStats,
  UsageDomainState,
} from './types'

/** 安装/更新剧本（演示数据）：describe-install 命令回显与运行输出模拟共用 */
export interface InstallRunScript {
  /** 基础安装命令（不含 --registry 尾巴，mock 按临时镜像开关拼接） */
  command: string
  /** 运行输出行（mock 逐行延时回显，模拟安装进程输出） */
  output: string[]
}

/** 本地目录导入演示条目（import-skill 无 path 时 mock 命中的模拟目录） */
export interface ImportSkillSeed {
  path: string
  name: string
}

/** devMock 容器（入口导出；SDK PluginDevMock 通用容器的插件自有形状） */
export interface AgentHubDevMock {
  /** 探测域种子：get-state 返回 + detect 动画的终态目标 */
  detection?: AgentHubState
  /** 安装域种子 + 运行剧本扩展 */
  install?: InstallDomainState & { runScripts?: Partial<Record<string, InstallRunScript>> }
  /** Skills 域种子 + 编辑器内容扩展 */
  skills?: SkillsDomainState & {
    skillContents?: Record<string, string>
    localImport?: ImportSkillSeed
  }
  /** 供应商域种子 + 导入发现/应用文件清单扩展 */
  providers?: ProvidersDomainState & {
    /** 反向导入演示：发现条目（guest 从 pi/opencode 配置反读，此处直接给结果） */
    importDiscoveries?: Array<{
      name: string
      baseUrl: string
      apiStyle: string
      models: string[]
      notes: string | null
    }>
    /** 反向导入 key 掩码（预设名 → 掩码，仅掩码形态） */
    importKeys?: Record<string, string>
    /** 应用预设时 guest 写入的文件清单（按目标） */
    applyFiles?: Record<string, string[]>
    /** claude 桥接冲突演示的桥接文件名 */
    applyBridges?: string[]
  }
  /** 使用统计域种子（状态 + 看板 + 会话列表 + 日志详情） */
  usage?: {
    state: UsageDomainState
    stats: UsageStats
    sessions: UsageSessionRow[]
    /** 按 session id 预置日志详情（未预置的 id 回退空事件流） */
    sessionDetails?: Record<number, UsageSessionDetail>
  }
}
