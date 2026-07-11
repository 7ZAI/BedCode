/**
 * AI Chatbox 插件内部类型定义
 */

/** API 格式 */
export type ApiFormat = 'openai' | 'anthropic' | 'gemini' | 'ollama'

/** API 提供商配置 */
export interface ApiProvider {
  id: string
  name: string
  apiKey: string
  baseUrl: string
  apiFormat: ApiFormat
  models: string[]
  activeModel: string
}

/** 聊天消息 */
export interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
}

/** 对话元数据 */
export interface ConversationMeta {
  id: string
  title: string
  createdAt: string
  updatedAt: string
  providerName: string
}

/** 预设模板 */
export interface ProviderPreset {
  name: string
  baseUrl: string
  apiFormat: ApiFormat
  models: string[]
}

/** AI 聊天响应事件 */
export interface CurrentInputEvent {
  sessionId: string
  text: string
}

/** 预设 Provider 模板 */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', apiFormat: 'openai', models: ['deepseek-chat', 'deepseek-reasoner'] },
  { name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', apiFormat: 'openai', models: ['gpt-4o-mini', 'gpt-4o', 'gpt-4-turbo'] },
  { name: 'Anthropic', baseUrl: 'https://api.anthropic.com', apiFormat: 'anthropic', models: ['claude-sonnet-4-20250514', 'claude-haiku-4-20250414'] },
  { name: 'Google Gemini', baseUrl: 'https://generativelanguage.googleapis.com/v1beta', apiFormat: 'gemini', models: ['gemini-2.0-flash', 'gemini-1.5-pro'] },
  { name: '通义千问', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', apiFormat: 'openai', models: ['qwen-turbo', 'qwen-plus', 'qwen-max'] },
  { name: 'Moonshot', baseUrl: 'https://api.moonshot.cn/v1', apiFormat: 'openai', models: ['moonshot-v1-8k', 'moonshot-v1-32k'] },
  { name: '智谱', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', apiFormat: 'openai', models: ['glm-4-flash', 'glm-4-plus', 'glm-4'] },
  { name: '硅基流动', baseUrl: 'https://api.siliconflow.cn/v1', apiFormat: 'openai', models: ['Qwen/Qwen2.5-7B-Instruct', 'deepseek-ai/DeepSeek-V3'] },
  { name: 'Ollama', baseUrl: 'http://localhost:11434', apiFormat: 'ollama', models: [] },
]

/** API 格式选项（用于下拉框） */
export const API_FORMAT_OPTIONS: { value: ApiFormat; label: string }[] = [
  { value: 'openai', label: 'OpenAI API' },
  { value: 'anthropic', label: 'Anthropic Messages' },
  { value: 'gemini', label: 'Google Gemini' },
  { value: 'ollama', label: 'Ollama' },
]

/** 生成简短 UUID */
export function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}
