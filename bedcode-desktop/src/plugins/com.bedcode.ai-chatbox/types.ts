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
