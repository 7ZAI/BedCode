/**
 * Agent Hub 前端类型（与 Rust 后端 wire 形状同构，camelCase）
 */

export type CliId = 'claude' | 'codex' | 'opencode' | 'pi'

/** 探测项状态：idle=未检测 detecting=进行中 ok/error=终态 */
export type DetectStatus = 'idle' | 'detecting' | 'ok' | 'not-installed' | 'error'

/** 单个 CLI 的探测结果（guest detect.rs 同构） */
export interface CliDetectInfo {
  installed: boolean | null
  version: string | null
  method: 'npm-global' | 'native' | 'standalone' | 'unknown'
  paths: string[]
  dual: boolean
  status: DetectStatus
  error: string | null
}

/** 开发环境采集（系统平台 + node/npm/pnpm 版本 + 当前 registry） */
export interface EnvInfo {
  /** 宿主平台名（std::env::consts::OS 值域：linux / windows / macos / …） */
  os: string | null
  node: string | null
  npm: string | null
  pnpm: string | null
  registry: string | null
}

/** 探测状态（host-storage 持久化 + `plugin:agent-hub:detection` 事件推送的完整形状） */
export interface AgentHubState {
  authGranted: boolean
  envStatus: DetectStatus
  envError?: string | null
  env: EnvInfo | null
  clis: Record<CliId, CliDetectInfo>
  /** 状态推送序号（探测期间多次推送全量状态，前端按序过滤乱序旧事件） */
  seq?: number
}

/** 面板分区（变体 B：顶部 pill 六段） */
export type HubTab = 'overview' | 'install' | 'skills' | 'providers' | 'stats' | 'logs'

// ==================== 安装/更新与镜像域（票据 03，guest install.rs 同构） ====================

/** 换源目标：候选源 URL（内置白名单 + 用户自定义，guest 端校验） */
export type MirrorTarget = string

/** 单个源测速结果（guest 端按耗时升序，最多前 10） */
export interface MirrorSourceSpeed {
  id: string
  url: string
  ms: number | null
  reachable: boolean
}

/** npm 源测速状态（多源列表，推荐 = 最快可达源） */
export interface SpeedTestState {
  status: 'idle' | 'testing' | 'ok' | 'error'
  sources: MirrorSourceSpeed[]
  recommend: string | null
  error: string | null
  testedAt: number | null
}

/** 用户自定义源（前端可增删，测速/换源白名单一并纳入） */
export interface CustomMirrorSource {
  id: string
  url: string
  addedAt?: number | null
}

/** ~/.npmrc 持久换源状态（文件内容不回传，仅 registry 值与备份标记） */
export interface NpmrcState {
  backupExists: boolean
  fileRegistry: string | null
  appliedAt?: number | null
  restoredAt?: number | null
}

/** 单家 CLI 的最新版本检查结果（outdated 由 guest 端比较） */
export interface CliUpdateInfo {
  latest: string | null
  outdated: boolean | null
  checkedAt: number | null
  error: string | null
}

/** 在途安装/更新 run */
export interface ActiveRun {
  runId: string
  cli: CliId
  action: 'install' | 'update'
  command: string
  useMirror: boolean
  startedAt: number
  cancelRequested: boolean
}

/** 最近一次安装/更新终态（output 为尾部截断后的回显） */
export interface LastRun {
  cli: CliId
  action: 'install' | 'update'
  command: string | null
  ok: boolean
  cancelled: boolean
  exitCode: number | null
  timedOut: boolean
  error: string | null
  output: string | null
  finishedAt: number
}

/** 安装域复合状态（host-storage `install` 键 + `plugin:agent-hub:install` 事件载荷） */
export interface InstallDomainState {
  active: ActiveRun | null
  last: LastRun | null
  updates: Record<CliId, CliUpdateInfo | null>
  mirror: {
    speed: SpeedTestState
    npmrc: NpmrcState
    /** 用户自定义源（add-custom-source / remove-custom-source 维护） */
    customSources?: CustomMirrorSource[]
  }
}

/** get-run-output 轮询返回（running 为在途尾部输出，终态回放 last） */
export interface RunOutput {
  status: 'idle' | 'running' | 'ok' | 'error' | 'cancelled'
  cli?: CliId
  action?: 'install' | 'update'
  command?: string | null
  output: string | null
}

// ==================== Skills 管理域（票据 04，guest skills.rs 同构） ====================

/** 单个 skill 库内文件（hash 为 FNV-1a hex；binary 文件 hash 为 null，分发按存在性比对） */
export interface SkillFile {
  path: string
  hash: string | null
}

/** 单分发目标的同步状态：none=未分发 distributed=全同步 stale=有缺失/落后 */
export type DistributionStatus = 'none' | 'distributed' | 'stale'

/** 单分发目标状态（missingFiles/staleFiles 为相对库内路径） */
export interface SkillDistribution {
  status: DistributionStatus
  missingFiles: string[]
  staleFiles: string[]
}

/** 规范库内单个 skill（guest skills.rs 同构） */
export interface SkillEntry {
  dir: string
  path: string
  name: string
  description: string | null
  allowedTools: string | null
  files: SkillFile[]
  hash: string
  error: string | null
  distribution: Record<'claude' | 'pi', SkillDistribution>
}

/** Skills 域复合状态（host-storage `skills` 键 + `plugin:agent-hub:skills` 事件载荷） */
export interface SkillsDomainState {
  status: 'idle' | 'scanning' | 'ready' | 'error'
  error: string | null
  scannedAt: number | null
  libraryRoot: string
  importing: boolean
  skills: SkillEntry[]
  targets: Record<'claude' | 'pi', { root: string; exists: boolean }>
  github: {
    last: {
      ok: boolean
      installed: string[]
      skippedFiles: number
      skipped: string[]
      error: string | null
      at: number
    } | null
  }
  import: {
    last: {
      ok: boolean
      name: string
      fileCount: number
      error: string | null
      at: number
    } | null
  }
}

/** read-skill 返回（编辑器装载载荷） */
export interface SkillDetail {
  dir: string
  name: string
  description: string | null
  allowedTools: string | null
  content: string
}

/** install-github-skill 返回（exists = 待覆盖确认的同名 skill） */
export interface GithubInstallResult {
  installed: boolean
  names?: string[]
  skippedFiles?: number
  skipped?: string[]
  error?: string
  exists?: string[]
}

/** import-skill 返回（picked/auth/exists 为交互分支信号） */
export interface ImportSkillResult {
  picked: boolean
  auth?: boolean
  path?: string
  name?: string
  exists?: boolean
}

// ==================== 供应商管理域（票据 05，guest providers.rs 同构） ====================

/** API 方言（与 chatbox ApiStyle 同构；custom 为逃生舱槽位） */
export type ApiStyle = 'openai' | 'anthropic' | 'gemini' | 'custom'

/** 应用目标（codex config.toml 官方格式未校准，v1 不开放） */
export type ProviderTarget = 'claude' | 'pi' | 'opencode'

/** 供应商预设（guest provider_preset 表同构；key 只以掩码 keyMask 出现） */
export interface ProviderPreset {
  id: number
  name: string
  baseUrl: string
  apiStyle: ApiStyle
  models: string[]
  /** key 掩码（前 3 字符 + 长度；"—" 表示未存 key）。明文只存 guest 插件库 */
  keyMask: string
  /** 来源标注（如 `pi:sensenova` / `opencode:gmi`），手工创建为 null */
  notes: string | null
  createdAt: number
  updatedAt: number
}

/** claude 只读视图：settings.json env 现状（token 掩码）+ 桥接文件存在性 */
export interface ClaudeEnvView {
  env: {
    baseUrl: string | null
    model: string | null
    authTokenMask: string | null
  }
  bridge: {
    providerConfigSh: boolean
    anthropicBridgeMjs: boolean
  }
}

/** 反向导入结果（keys 为预设名 → key 掩码，仅掩码形态） */
export interface ImportProvidersResult {
  created: string[]
  skipped: string[]
  keys: Record<string, string>
}

/** 保存预设结果（nameExists = 同名冲突） */
export interface SavePresetResult {
  saved: boolean
  nameExists?: boolean
}

/** 应用预设返回（bridgeConflict = claude 桥接冲突，待用户确认 force） */
export interface ApplyProviderResult {
  applied: boolean
  files?: string[]
  bridgeConflict?: boolean
  bridges?: string[]
  error?: string
}

/**
 * key 提供方式（apply 时四选一）：stored 中心库已存 / inline 现场输入 /
 * source 内存直拷 / none 保留目标既有凭据；claude 桥接确认走独立的顶层
 * force 标志，不占用 kind（guest 端 force 与 key 模式正交）
 */
export interface ApplyKeySpec {
  kind: 'inline' | 'stored' | 'source' | 'none'
  value?: string
  cli?: string
  provider?: string
}

/** 供应商域复合状态（storage `providers` 键 + `plugin:agent-hub:providers` 事件载荷） */
export interface ProvidersDomainState {
  presets: ProviderPreset[]
  claude: ClaudeEnvView
  import: {
    last: {
      ok: boolean
      created: string[]
      skipped: string[]
      keys: Record<string, string>
      error: string | null
      at: number
    } | null
  }
  apply: {
    last: {
      ok: boolean
      preset: string | null
      target: ProviderTarget | null
      files: string[]
      keyMode: string
      keyLen: number | null
      error: string | null
      at: number
    } | null
  }
}

// ==================== 使用统计与会话日志域（票据 06，guest usage.rs 同构） ====================

/** 适配器扫描分项（files=枚举文件数 parsed=本次解析 skipped=水位未变/超限跳过） */
export interface UsageAdapterStat {
  files: number
  parsed: number
  skipped: number
  sessions: number
  error: string | null
}

/** 使用统计域状态（host-storage `usage` 键 + `plugin:agent-hub:usage` 事件载荷） */
export interface UsageDomainState {
  status: 'idle' | 'syncing' | 'ok' | 'error' | 'auth-required'
  error: string | null
  syncedAt: number | null
  authGranted: boolean
  /** 用户主目录（项目路径 ~ 折叠展示用） */
  home: string
  adapters: Record<'claude' | 'pi', UsageAdapterStat> & Record<string, UsageAdapterStat>
  /** 日志来源清单（内置只读 + 自定义增删；含各来源扫描计数） */
  sources: UsageSource[]
  /** 正在使用的项目会话（扫描时计算：claude 读 ~/.claude.json 配置权威，
   *  pi 取最新会话；键=适配器，null=无） */
  activeSessions?: Partial<Record<string, { project: string | null; session_id: string } | null>>
}

/** 日志来源条目（wire 与 list-usage-sources 返回行同构） */
export interface UsageSource {
  name: string
  /** 绝对路径（展示时前端折叠 ~ 前缀） */
  path: string
  builtin: boolean
  /** 扫描计数（与 adapters[name] 同源，list-usage-sources 合并注入） */
  scan?: UsageAdapterStat
}

/** 会话聚合记录（usage_session 表行，wire 为 DB 列名 snake_case） */
export interface UsageSessionRow {
  id: number
  adapter: CliId
  cli_session_id: string
  project: string | null
  title: string | null
  started_at: number | null
  ended_at: number | null
  duration_ms: number | null
  model: string | null
  tokens_in: number
  tokens_out: number
  tokens_cache_read: number
  tokens_cache_write: number
  tokens_reasoning: number
  cost_total: number | null
  /** 是否正在使用的项目会话（扫描时按配置/最新会话标记；仅列表接口注入） */
  active?: boolean
  /** 仅 read-usage-session 返回（列表接口不带）；源 JSONL 路径 */
  source_path?: string | null
}

/** 看板聚合（get-usage-stats 返回；day/project/model 为分组行） */
export interface UsageStats {
  total: {
    sessions: number
    tokens_in: number
    tokens_out: number
    tokens_cache_read: number
    tokens_cache_write: number
    tokens_reasoning: number
    duration_ms: number
    cost_total: number | null
  }
  byDay: { day: string; sessions: number; tokens_in: number; tokens_out: number }[]
  byCli: (GroupStatRow & { adapter: string })[]
  byProject: (GroupStatRow & { project: string | null })[]
  byModel: {
    model: string
    sessions: number
    messages: number
    tokens_in: number
    tokens_out: number
  }[]
}

/** 分组统计行（按 CLI / 按项目共用形状，key 字段二选一） */
export interface GroupStatRow {
  sessions: number
  duration_ms: number
  tokens_in: number
  tokens_out: number
  tokens_cache_read?: number
  tokens_cache_write?: number
  tokens_reasoning?: number
  cost_total: number | null
}

/** 会话列表分页载荷（list-usage-sessions 返回） */
export interface UsageSessionPage {
  sessions: UsageSessionRow[]
  total: number
  offset: number
  limit: number
}

/** 归一事件（read-usage-session 返回行；tokens 仅助手事件携带） */
export interface NormalizedEventView {
  ts: number | null
  role: 'user' | 'assistant' | 'tool' | 'system'
  text: string
  model: string | null
  tokens: {
    input: number
    output: number
    cacheRead: number
    cacheWrite: number
    reasoning: number
  } | null
}

/** 会话日志视图载荷（read-usage-session 返回） */
export interface UsageSessionDetail {
  session: UsageSessionRow
  events: NormalizedEventView[]
  eventsTruncated: boolean
  raw: string[]
  rawTruncated: boolean
  skippedLines: number
}
