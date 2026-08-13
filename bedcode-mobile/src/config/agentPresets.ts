/**
 * Agent CLI 命令预设
 *
 * 按 Agent CLI 分组的内置快捷命令集（见 CONTEXT.md「命令预设」术语）。
 * 预设内置移动端本地、随 App 版本演进；Agent CLI 识别由移动端对会话配置的
 * 启动命令（command 字段）关键词包含匹配得出，识别结果与手动覆盖存移动端
 * 本地 JSON 文件（settings DB，与 custom_commands 同模式，见 ADR-0014）。
 */

/** Agent CLI 标识（命令预设的加载键，generic = 未识别，不加载预设） */
export type AgentType = 'claude_code' | 'pi' | 'codex' | 'opencode' | 'generic'

/** 快捷命令点击模式：发送 = 文本发到终端输入行不带回车；执行 = 文本 + Enter */
export type CommandMode = 'send' | 'execute'

/** 预设命令项（加载时由 store 附加 id 与 builtin 标记） */
export interface PresetCommand {
  command: string
  mode: CommandMode
}

/** 除 generic 外的 Agent CLI 枚举（generic 无预设） */
export const AGENT_TYPES: Exclude<AgentType, 'generic'>[] = ['claude_code', 'pi', 'codex', 'opencode']

/**
 * 各 Agent CLI 的快捷命令预设（每套 12 条：新会话/换会话/skills/压缩/查看上下文
 * 5 类必选 + 7 条该 CLI 高频命令；skills 位为发送模式，其余执行）
 */
export const AGENT_PRESETS: Record<Exclude<AgentType, 'generic'>, PresetCommand[]> = {
  claude_code: [
    { command: '/clear', mode: 'execute' }, // 创建新会话（别名 /new /reset）
    { command: '/resume', mode: 'execute' }, // 更换会话
    { command: '/compact', mode: 'execute' }, // 压缩上下文
    { command: '/context', mode: 'execute' }, // 查看上下文（可视化 grid）
    { command: '/', mode: 'send' }, // skills：打开命令/技能补全，用户补全后提交
    { command: '/model', mode: 'execute' },
    { command: '/code-review', mode: 'execute' },
    { command: '/permissions', mode: 'execute' },
    { command: '/cost', mode: 'execute' },
    { command: '/init', mode: 'execute' },
    { command: '/memory', mode: 'execute' },
    { command: '/config', mode: 'execute' },
  ],
  pi: [
    { command: '/new', mode: 'execute' }, // 创建新会话
    { command: '/resume', mode: 'execute' }, // 更换会话
    { command: '/compact', mode: 'execute' }, // 压缩上下文
    { command: '/session', mode: 'execute' }, // 查看上下文（messages/tokens/cost）
    { command: '/skill:', mode: 'send' }, // skills：前缀式调用，用户补全技能名
    { command: '/model', mode: 'execute' },
    { command: '/tree', mode: 'execute' },
    { command: '/settings', mode: 'execute' },
    { command: '/reload', mode: 'execute' },
    { command: '/fork', mode: 'execute' },
    { command: '/share', mode: 'execute' },
    { command: '/hotkeys', mode: 'execute' },
  ],
  codex: [
    { command: '/new', mode: 'execute' }, // 创建新会话（同 session 新对话）
    { command: '/resume', mode: 'execute' }, // 更换会话
    { command: '/compact', mode: 'execute' }, // 压缩上下文
    { command: '/status', mode: 'execute' }, // 查看上下文（会话配置 + token 用量）
    { command: '/init', mode: 'execute' }, // skills 替代：codex 无 skills 命令，AGENTS.md 引导
    { command: '/model', mode: 'execute' },
    { command: '/permissions', mode: 'execute' },
    { command: '/plan', mode: 'execute' },
    { command: '/review', mode: 'execute' },
    { command: '/diff', mode: 'execute' },
    { command: '/keymap', mode: 'execute' },
    { command: '/quit', mode: 'execute' },
  ],
  opencode: [
    { command: '/new', mode: 'execute' }, // 创建新会话（别名 /clear）
    { command: '/sessions', mode: 'execute' }, // 更换会话（别名 /resume /continue）
    { command: '/compact', mode: 'execute' }, // 压缩上下文（别名 /summarize）
    { command: '/details', mode: 'execute' }, // 查看上下文替代：切换工具执行细节
    { command: '/templates', mode: 'execute' }, // skills 替代：opencode 无 skills 命令，模板最接近
    { command: '/models', mode: 'execute' },
    { command: '/undo', mode: 'execute' },
    { command: '/redo', mode: 'execute' },
    { command: '/init', mode: 'execute' },
    { command: '/share', mode: 'execute' },
    { command: '/exit', mode: 'execute' },
    { command: '/help', mode: 'execute' },
  ],
}

/**
 * 获取某 Agent CLI 预设的命令文本列表（`/` 补全数据源）
 *
 * generic（未识别）无预设，返回空列表；调用方据此决定是否启用补全弹层。
 */
export function getPresetCommandTexts(type: AgentType): string[] {
  if (type === 'generic' || !AGENT_PRESETS[type]) return []
  return AGENT_PRESETS[type].map(c => c.command)
}

/**
 * 全部 Agent CLI 预设命令合集（去重，按预设分组顺序）：`/` 补全数据源
 *
 * 与单会话预设（AGENT_PRESETS）不同，补全展示所有 Agent CLI 的命令，
 * 便于跨 CLI 探索；generic（未识别）会话同样可用。
 */
export function getAllPresetCommandTexts(): string[] {
  const seen = new Set<string>()
  const result: string[] = []
  for (const type of AGENT_TYPES) {
    for (const c of AGENT_PRESETS[type]) {
      if (!seen.has(c.command)) {
        seen.add(c.command)
        result.push(c.command)
      }
    }
  }
  return result
}

/**
 * `/` 补全过滤：返回以输入前缀开头（大小写不敏感）的命令文本
 *
 * 与 agent 内部补全同构（前缀匹配），但走本地数据零延迟；
 * 排除裸 `/`（补全自身无意义，skills 入口仍走快捷键面板的发送模式）。
 */
export function filterPresetCommands(commands: string[], input: string): string[] {
  const trimmed = input.trim()
  if (!trimmed.startsWith('/')) return []
  const keyword = trimmed.slice(1).toLowerCase()
  return commands.filter(c => c.length > 1 && c.toLowerCase().startsWith(`/${keyword}`))
}

/** 会话启动命令 → Agent CLI 关键词包含匹配（命中即返回，按序优先） */
const AGENT_KEYWORDS: Array<[Exclude<AgentType, 'generic'>, string]> = [
  ['claude_code', 'claude'],
  ['codex', 'codex'],
  ['opencode', 'opencode'],
  ['pi', 'pi'],
]

/** 从会话启动命令识别 Agent CLI；未命中返回 generic */
export function detectAgentType(command: string): AgentType {
  const lower = command.toLowerCase()
  for (const [type, keyword] of AGENT_KEYWORDS) {
    if (lower.includes(keyword)) return type
  }
  return 'generic'
}
