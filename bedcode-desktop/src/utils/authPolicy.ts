/**
 * 应用授权读模型的类型与展示助手
 *
 * 数据源是宿主命令 `plugin_auth_overview`（spec §9.3：策略 + 记录 + 第一方免询问项，
 * 设置页总览与应用详情页共用）。本文件只承担两件 UI 决策：
 *
 * 1. **风险排序**：`always_allow`（免询问自动放行）置顶，`always_ask`（每次询问）
 *    最保守排末（spec §9.1 只规定置顶项，其余按保守度排列）；
 * 2. **缺项兜底**：读模型缺该资源策略或值未识别时按 `default` 处理——与宿主
 *    `AuthStrategy::parse` 的 fail-safe 同向（不认识 ≠ 更宽松的档位）。
 *
 * 判定语义不在这里复制：真正决定「问不问 / 放不放」的只有宿主授权策略层；
 * 前端不依据这些字段做任何放行决策。
 */

/** 受管资源类别（与宿主 `AuthResource` 的 wire 值一致） */
export const AUTH_RESOURCES = ['fs', 'network'] as const
export type AuthResource = (typeof AUTH_RESOURCES)[number]

/** 授权策略档位（spec §4.1） */
export type AuthStrategy = 'always_ask' | 'default' | 'always_allow'

/**
 * 策略档位的展示顺序（保守 → 宽松），策略控件按此渲染
 *
 * 与 [`STRATEGY_RANK`]（风险权重，值越小越宽松，用于列表置顶）方向相反：
 * 控件按用户心智排（先看到最保守的），风险排序按严重度排。顺序只在这里定义一次，
 * 两处各写一套会在新增档位时立刻不一致。
 */
export const STRATEGY_TIERS: readonly AuthStrategy[] = ['always_ask', 'default', 'always_allow']

/** 某资源上的策略取值（读模型固定两类资源各一条） */
export interface ResourceStrategy {
  resource: string
  strategy: string
}

/** 授权记录来源：用户确认 / 免询问自动放行 / 旧版遗留 / 用户拒绝 */
export const AUTH_RECORD_SOURCES = ['user', 'always_allow', 'legacy', 'user_deny'] as const
export type AuthRecordSource = (typeof AUTH_RECORD_SOURCES)[number]

/** 一条授权记录（`effect` / `resource` / `source` 保持开放字符串：宿主将来加值不炸前端） */
export interface AuthRecord {
  id: number
  pluginId: string
  resource: string
  /** fs: 规范路径前缀；network: `scheme://host:port[/path-prefix]` */
  target: string
  effect: string
  /** fs: `["read"]` / `["read","write"]`；network 恒空 */
  ops: string[]
  prefixMatch: boolean
  source: string
  createdAt: number
}

/** 第一方免询问项（宿主 `fs_auth::FIRST_PARTY_TRUSTED_DIRS` 的只读投影） */
export interface FirstPartyDirEntry {
  pluginId: string
  /** `home`：家目录下前缀；`project-segment`：任意项目下的具名目录段 */
  kind: string
  /** 清单原始值：`~/.agents` 的 `.agents`、项目段 `.claude` */
  value: string
}

/** 单个应用的授权读模型 */
export interface PluginAuthOverview {
  pluginId: string
  name: string
  strategies: ResourceStrategy[]
  records: AuthRecord[]
  firstPartyDirs: FirstPartyDirEntry[]
}

/** 某应用的某资源授权记录（读模型已按资源分区，这里只做筛选） */
export function recordsOf(app: PluginAuthOverview, resource: string): AuthRecord[] {
  return app.records.filter((record) => record.resource === resource)
}

// ==================== 四分区（spec §9.2：详情页授权记录区块） ====================

/**
 * 详情页「授权记录」区块的四分区：用户已授权 / 免询问自动放行 / 内置免询问 / 硬拒绝
 *
 * 分区口径与宿主记录来源一一对应（来源 + 效果，不按资源——一个分区可同时含文件
 * 与网络记录）：
 * - 用户已授权：`allow` 且来源为 `user`（弹窗确认）或 `legacy`（旧版扁平授权回退）
 * - 免询问自动放行：`allow` 且来源为 `always_allow`（策略层自动放行落账，UI 标「未经确认」）
 * - 内置免询问：读模型 `firstPartyDirs`（第一方清单投影，非记录行）
 * - 硬拒绝：`deny`（来源 `user_deny`）
 *
 * 未知来源的 allow 记录归「用户已授权」并按**未知来源**展示原文（不误标成免询问
 * 自动放行——溯源标错比标丑严重）；排序按落账时间（稳定输出）。
 */

/** 用户已授权（allow + user/legacy；未知来源的 allow 按保守口径归本分区） */
export function userGrantedRecords(app: PluginAuthOverview): AuthRecord[] {
  return app.records
    .filter(
      (r) => effectKeySuffix(r.effect) === 'allow' && sourceKeySuffix(r.source) !== 'always_allow',
    )
    .sort((a, b) => a.createdAt - b.createdAt)
}

/** 免询问自动放行（allow + always_allow，必须带「未经确认」标记，spec §9.4） */
export function autoAllowedRecords(app: PluginAuthOverview): AuthRecord[] {
  return app.records
    .filter(
      (r) => effectKeySuffix(r.effect) === 'allow' && sourceKeySuffix(r.source) === 'always_allow',
    )
    .sort((a, b) => a.createdAt - b.createdAt)
}

/** 硬拒绝（effect=deny） */
export function deniedRecords(app: PluginAuthOverview): AuthRecord[] {
  return app.records
    .filter((r) => effectKeySuffix(r.effect) === 'deny')
    .sort((a, b) => a.createdAt - b.createdAt)
}

/**
 * 内置免询问项的可展示路径形态（第一方清单投影，kind + value 交前端组合）
 *
 * - `home`：`~/.agents` 的 `~/` 前缀形态（撤销时按此传给宿主，宿主展开为绝对路径）
 * - `project-segment`：任意项目下的具名目录段（项目根由用户每次选，落不成可撤销的
 *   具体路径——详情页仅展示，撤销语义见票 08）
 */
export function firstPartyLabel(entry: FirstPartyDirEntry): string {
  return entry.kind === 'home' ? `~/` + entry.value : `<project>/` + entry.value
}

/** 某应用的内置免询问项（读模型已按 plugin_id 过滤归属） */
export function firstPartyDirsOf(app: PluginAuthOverview): FirstPartyDirEntry[] {
  return app.firstPartyDirs
}

/**
 * 记录来源 → i18n key 后缀；**未知来源返回 null**
 *
 * 未知值由界面回落显示原文：宿主将来加来源取值时宁可显示 `always_allow_v2`
 * 这种生值，也不能错标成「用户确认」——授权溯源标错比标丑严重得多。
 */
export function sourceKeySuffix(raw: string): AuthRecordSource | null {
  return (AUTH_RECORD_SOURCES as readonly string[]).includes(raw)
    ? (raw as AuthRecordSource)
    : null
}

/** 记录效果 → i18n key 后缀（`allow` / `deny`）；未知值返回 null 由界面回落原文 */
export function effectKeySuffix(raw: string): 'allow' | 'deny' | null {
  return raw === 'allow' || raw === 'deny' ? raw : null
}

/**
 * 操作集 → i18n key 后缀（`read` / `write` / `read_write`）；空集（网络记录）返回 null
 *
 * 展示口径与宿主 `FsOps::as_wire_str` 一致：读 + 写 = 读写；只有单个操作时按单个标。
 */
export function opsKeySuffix(ops: string[]): 'read' | 'write' | 'read_write' | null {
  const hasRead = ops.includes('read')
  const hasWrite = ops.includes('write')
  if (hasRead && hasWrite) return 'read_write'
  if (hasRead) return 'read'
  if (hasWrite) return 'write'
  return null
}

/** 档位风险权重：始终允许置顶，默认次之，总是询问最保守排末（spec §9.1） */
const STRATEGY_RANK: Record<AuthStrategy, number> = {
  always_allow: 0,
  default: 1,
  always_ask: 2,
}

/** 未知档位值 / 缺项一律归一为默认档（与宿主 fail-safe 同向） */
export function normalizeStrategy(raw: string | undefined): AuthStrategy {
  return raw === 'always_ask' || raw === 'always_allow' ? raw : 'default'
}

/** 取某资源的策略（读模型缺该资源时按默认档） */
export function strategyOf(app: PluginAuthOverview, resource: string): AuthStrategy {
  return normalizeStrategy(app.strategies.find((s) => s.resource === resource)?.strategy)
}

/** 某资源的授权记录条数（总览徽标） */
export function recordCount(app: PluginAuthOverview, resource: string): number {
  return app.records.filter((r) => r.resource === resource).length
}

/**
 * 应用的风险权重：取两类资源里风险最高的那一档
 *
 * 任一资源为 `always_allow` 即置顶（spec §9.1 / 票 06）。
 */
export function riskRank(app: PluginAuthOverview): number {
  return Math.min(...AUTH_RESOURCES.map((resource) => STRATEGY_RANK[strategyOf(app, resource)]))
}

/** 文本比较：不依赖 locale（避免不同环境排序不同导致测试不稳定） */
function compareText(a: string, b: string): number {
  if (a === b) return 0
  return a < b ? -1 : 1
}

/** 按风险排序（不改入参）；同档按名称、名称相同按 id 兜底，保证顺序确定 */
export function sortAppsByRisk(apps: PluginAuthOverview[]): PluginAuthOverview[] {
  return [...apps].sort(
    (a, b) =>
      riskRank(a) - riskRank(b) ||
      compareText(a.name, b.name) ||
      compareText(a.pluginId, b.pluginId),
  )
}
