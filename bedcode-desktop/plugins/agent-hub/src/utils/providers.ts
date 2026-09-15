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

/** 应用目标（与 guest apply 白名单同构；codex 待格式校准后开放） */
export const APPLY_TARGETS = ['claude', 'pi', 'opencode'] as const

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
