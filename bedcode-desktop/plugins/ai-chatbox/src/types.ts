/**
 * AI Chatbox 插件内部类型定义
 */

/** API 格式（当前仅 OpenAI 兼容协议，字段保留供未来扩展） */
export type ApiFormat = 'openai'

/** API 提供商配置（storage 持久化；providers.json 为数据目录内镜像占位） */
export interface ApiProvider {
  id: string
  name: string
  apiKey: string
  baseUrl: string
  apiFormat: ApiFormat
  models: string[]
  activeModel: string
  /** 创建来源的预设模板 id（旧数据缺失时走首字母头像，向后兼容） */
  presetId?: string
  /** 对话级临时模型覆盖（发给 Rust 时优先于 activeModel；不持久化） */
  model?: string
}

/** token 用量（流结束事件由宿主从 SSE usage 透传） */
export interface Usage {
  promptTokens: number
  completionTokens: number
  totalTokens: number
}

/** 聊天消息（assistant 消息含 model / usage） */
export interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
  model?: string
  usage?: Usage
}

/** 对话元数据（对话文件首行 + index.jsonl） */
export interface ConversationMeta {
  id: string
  title: string
  createdAt: string
  updatedAt: string
  providerId: string
  providerName: string
  model: string
  systemPrompt: string
}

/** 预设模板 id（与 src/assets/providers/ 下品牌图标一一对应） */
export type PresetId = 'deepseek' | 'qwen' | 'openai' | 'anthropic'

/** 供应商预设模板（只读添加起点，不进入供应商列表） */
export interface ProviderPreset {
  id: PresetId
  name: string
  baseUrl: string
  models: string[]
}

/** 内置供应商预设（全部走 OpenAI 兼容协议；Anthropic 官方 OpenAI 兼容端点） */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  { id: 'deepseek', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', models: ['deepseek-chat', 'deepseek-reasoner'] },
  { id: 'qwen', name: '通义千问 (Qwen)', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', models: ['qwen-turbo', 'qwen-plus', 'qwen-max'] },
  { id: 'openai', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', models: ['gpt-4o-mini', 'gpt-4o', 'gpt-4-turbo'] },
  { id: 'anthropic', name: 'Anthropic', baseUrl: 'https://api.anthropic.com/v1', models: ['claude-sonnet-4-20250514', 'claude-haiku-4-20250414'] },
]

/** 生成简短 ID（时间戳 + 随机段，对话/供应商/流共用） */
export function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}
