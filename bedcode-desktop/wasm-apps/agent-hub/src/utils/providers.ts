/**
 * 供应商预设内置模板与来源解析（票据 05）
 *
 * 四套内置模板复用 chatbox `PROVIDER_PRESETS`（全部走 OpenAI 兼容协议，
 * Anthropic 为官方 OpenAI 兼容端点；本插件面向 CLI 供应商场景，anthropic
 * 模板保留 Anthropic 官方方言端点，由 apiStyle 决定写入形态）。
 * custom 为空白起点（逃生舱槽位）。
 */
import type { ApiStyle } from '../types'

/** 内置模板（新建预设的起点，不进入预设列表） */
export interface ProviderTemplate {
  id: 'deepseek' | 'qwen' | 'openai' | 'anthropic' | 'custom'
  name: string
  baseUrl: string
  apiStyle: ApiStyle
  models: string[]
}

export const PROVIDER_TEMPLATES: ProviderTemplate[] = [
  {
    id: 'deepseek',
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    apiStyle: 'openai',
    models: ['deepseek-chat', 'deepseek-reasoner'],
  },
  {
    id: 'qwen',
    name: '通义千问 (Qwen)',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    apiStyle: 'openai',
    models: ['qwen-turbo', 'qwen-plus', 'qwen-max'],
  },
  {
    id: 'openai',
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    apiStyle: 'openai',
    models: ['gpt-4o-mini', 'gpt-4o', 'gpt-4-turbo'],
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    baseUrl: 'https://api.anthropic.com/v1',
    apiStyle: 'anthropic',
    models: ['claude-sonnet-4-20250514', 'claude-haiku-4-20250414'],
  },
  {
    id: 'custom',
    name: '',
    baseUrl: '',
    apiStyle: 'openai',
    models: [],
  },
]

/** 应用目标（与 guest apply 白名单同构；codex = 登记 provider + 设为当前模型） */
export const APPLY_TARGETS = ['claude', 'pi', 'opencode', 'codex'] as const

/** 各目标的配置文件落点（面板提示用；与 guest paths.rs 同构） */
export const TARGET_PATHS: Record<string, string> = {
  claude: '~/.claude/settings.json',
  pi: '~/.pi/agent/{models.json, auth.json}',
  opencode: '~/.config/opencode/opencode.json',
  codex: '~/.codex/config.toml',
}

/**
 * 由预设名派生 codex 的 `env_key` 变量名（`<NAME>_API_KEY` 大写）
 *
 * codex 配置里只写**变量名**，真值由用户 shell 环境提供——所以这里只给一个
 * 符合惯例的默认值，用户可改（如供应商文档指定了别的变量名）。
 */
export function deriveEnvKeyName(presetName: string): string {
  const cleaned = presetName
    .trim()
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toUpperCase()
  if (!cleaned) return 'API_KEY'
  // 以数字开头的名（如“360 网关”）不是合法 shell 变量名，guest 会拒；加下划线前缀
  return `${/^[0-9]/.test(cleaned) ? '_' : ''}${cleaned}_API_KEY`
}

/**
 * 由 baseUrl 派生模型查询 URL（`{base}/models`）
 *
 * 用户可手填覆盖（不少网关的 /models 不在 base 下，如 Gemini 的
 * `.../v1beta/models`）。已以 `/models` 结尾时原样返回（不叠成 `/models/models`）。
 */
export function defaultModelsUrl(baseUrl: string): string {
  const base = baseUrl.trim().replace(/\/+$/, '')
  if (!base) return ''
  if (/\/models$/.test(base)) return base
  return `${base}/models`
}

/** 文本域 → 模型 id 列表（去空白行、去重保序；与 guest 侧 trim/filter 同语义） */
export function parseModelsText(text: string): string[] {
  const out: string[] = []
  for (const line of text.split('\n')) {
    const id = line.trim()
    if (id && !out.includes(id)) out.push(id)
  }
  return out
}

/** 模型 id 列表 → 文本域（尾随换行不产生空行） */
export function modelsToText(models: string[]): string {
  return models.join('\n')
}

/**
 * 合并模型列表（查询结果 ∪ 已有，手动条目不被抹掉）
 *
 * 顺序：已有在前（用户手工整理的顺序不变），新查询到的追加在后。
 */
export function mergeModelIds(existing: string[], incoming: string[]): string[] {
  const out = [...existing]
  for (const id of incoming) {
    const trimmed = id.trim()
    if (trimmed && !out.includes(trimmed)) out.push(trimmed)
  }
  return out
}

/** 从预设 notes（`pi:sensenova` / `opencode:gmi`）解析 key 直拷源 */
export function sourceFromNotes(
  notes: string | null | undefined,
): { cli: string; provider: string } | null {
  if (!notes) return null
  const idx = notes.indexOf(':')
  if (idx <= 0) return null
  const cli = notes.slice(0, idx).trim()
  const provider = notes.slice(idx + 1).trim()
  if (!cli || !provider) return null
  if (cli !== 'pi' && cli !== 'opencode' && cli !== 'claude') return null
  return { cli, provider }
}
