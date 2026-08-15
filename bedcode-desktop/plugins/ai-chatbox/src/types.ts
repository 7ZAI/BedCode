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

/** 聊天消息（assistant 消息含 model / usage；reasoning 为思考过程全文，P3 随日志落盘） */
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

/** 思考模式（插件级全局配置：default=不传参跟随模型；enabled/disabled 强制开/关） */
export type ThinkingMode = 'default' | 'enabled' | 'disabled'

/** 推理强度（DeepSeek `reasoning_effort` 语义；仅 thinkingMode=enabled 时写入请求） */
export type ReasoningEffort = 'low' | 'high' | 'max'

/** 代码块行距（数字，范围 [MIN, MAX]；配置归一化与 plugin.json 保持一致） */
export type CodeLineHeight = number
/** 行距可调范围与默认值 */
export const CODE_LINE_HEIGHT_MIN = 0.5
export const CODE_LINE_HEIGHT_MAX = 2
export const DEFAULT_CODE_LINE_HEIGHT = 1.6

/** 代码块高亮主题：auto=跟随宿主深浅色；light/dark=通用浅深色；
 * github-light/github-dark/dracula=具名风格主题（桌面端 hljs 变量组 / 移动端 Shiki 主题包） */
export type CodeTheme = 'auto' | 'light' | 'dark' | 'github-light' | 'github-dark' | 'dracula'

/** 代码块字体大小范围（px；配置归一化与 plugin.json 保持一致） */
export const CODE_FONT_SIZE_MIN = 11
export const CODE_FONT_SIZE_MAX = 18

/** 插件级全局配置（contributes.configuration，storage key `config`；
    宿主配置页保存的值可能缺项，读取侧必须合并默认值） */
export interface PluginConfig {
  thinkingMode: ThinkingMode
  reasoningEffort: ReasoningEffort
  showReasoning: boolean
  /** 代码块行距（0.5-2.0，默认 1.6） */
  codeLineHeight: number
  /** 代码块字体大小（px） */
  codeFontSize: number
  /** 代码块高亮主题 */
  codeTheme: CodeTheme
}

/** 配置默认值（与 plugin.json configuration 的 default 字段保持一致） */
export const DEFAULT_PLUGIN_CONFIG: PluginConfig = {
  thinkingMode: 'default',
  reasoningEffort: 'high',
  showReasoning: true,
  codeLineHeight: DEFAULT_CODE_LINE_HEIGHT,
  codeFontSize: 13,
  codeTheme: 'auto',
}

/** 生成简短 ID（时间戳 + 随机段，对话/供应商/流共用） */
export function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}
