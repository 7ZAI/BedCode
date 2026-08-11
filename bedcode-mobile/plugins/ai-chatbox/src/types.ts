/**
 * AI Chatbox 插件内部类型定义
 */

/** API 协议方言（供应商适配层分派键；custom 为私有网关逃生舱槽位，本期不实现 UI） */
export type ApiStyle = 'openai' | 'anthropic' | 'gemini' | 'custom'

/** API 提供商配置（storage 持久化；providers.json 为数据目录内镜像占位） */
export interface ApiProvider {
  id: string
  name: string
  apiKey: string
  baseUrl: string
  apiStyle: ApiStyle
  models: string[]
  activeModel: string
  /** 对话级临时模型覆盖（发给 Rust 时优先于 activeModel；不持久化） */
  model?: string
}

/** token 用量（流结束事件由宿主从 SSE usage 透传） */
export interface Usage {
  promptTokens: number
  completionTokens: number
  totalTokens: number
}

/** 聊天消息（assistant 消息含 model / usage；reasoning 为思考过程全文，随日志落盘） */
export interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
  model?: string
  usage?: Usage
  reasoning?: string
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

/** 供应商预设目录 */
export interface ProviderPreset {
  name: string
  baseUrl: string
  models: string[]
}

/** 内置供应商预设（全部走 OpenAI 兼容协议；Anthropic 官方 OpenAI 兼容端点） */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', models: ['deepseek-chat', 'deepseek-reasoner'] },
  { name: '通义千问 (Qwen)', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', models: ['qwen-turbo', 'qwen-plus', 'qwen-max'] },
  { name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', models: ['gpt-4o-mini', 'gpt-4o', 'gpt-4-turbo'] },
  { name: 'Anthropic', baseUrl: 'https://api.anthropic.com/v1', models: ['claude-sonnet-4-20250514', 'claude-haiku-4-20250414'] },
]

/** 思考模式（插件级全局配置：default=不传参跟随模型；enabled/disabled 强制开/关） */
export type ThinkingMode = 'default' | 'enabled' | 'disabled'

/** 推理强度（DeepSeek `reasoning_effort` 语义；仅 thinkingMode=enabled 时写入请求） */
export type ReasoningEffort = 'low' | 'high' | 'max'

/** 插件级全局配置（contributes.configuration，storage key `config`；
    宿主配置页保存的值可能缺项，读取侧必须合并默认值） */
export interface PluginConfig {
  thinkingMode: ThinkingMode
  reasoningEffort: ReasoningEffort
  showReasoning: boolean
}

/** 配置默认值（与 plugin.json configuration 的 default 字段保持一致） */
export const DEFAULT_PLUGIN_CONFIG: PluginConfig = {
  thinkingMode: 'default',
  reasoningEffort: 'high',
  showReasoning: true,
}

/** 生成简短 ID（时间戳 + 随机段，对话/供应商/流共用） */
export function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}
