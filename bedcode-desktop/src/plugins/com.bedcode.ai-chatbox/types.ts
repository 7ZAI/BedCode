/**
 * AI Chatbox 插件内部类型定义
 */

/** API 提供商配置 */
export interface ApiProvider {
  name: string
  apiKey: string
  baseUrl: string
  model: string
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
  model: string
}

/** AI 聊天响应事件 */
export interface CurrentInputEvent {
  sessionId: string
  text: string
}

/** 预设 Provider 模板（从 openaiClient.ts 迁移） */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { name: '通义千问', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-turbo' },
  { name: 'Moonshot', baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  { name: '智谱', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4-flash' },
  { name: '硅基流动', baseUrl: 'https://api.siliconflow.cn/v1', model: 'Qwen/Qwen2.5-7B-Instruct' },
]
