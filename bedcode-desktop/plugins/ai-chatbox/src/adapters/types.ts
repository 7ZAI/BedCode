/**
 * 协议适配层类型定义
 *
 * 供应商方言差异全部收敛于此（ADR-0010）：请求构建 + 流式/完整响应解析，
 * 宿主与 SDK 零改动。流式统一走 raw 模式（sseFormat:""）+ 前端 SSE 缓冲解析。
 */
import type { ApiProvider, ApiStyle, ReasoningEffort, ThinkingMode, Usage } from '../types'

/** 请求消息（仅 role/content，不携带 timestamp/reasoning 等本地字段） */
export interface AdapterMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

/** 部分 token 用量（anthropic 分事件携带，字段可缺省，由前端 mergeUsage 合并） */
export type PartialUsage = Partial<Usage>

/** 流事件解析结果（各字段可缺省；done 标记流终结，宿主 done 事件仅作兜底） */
export interface StreamEvent {
  chunk?: string
  reasoning?: string
  usage?: PartialUsage
  done?: boolean
}

/** 宿主 http_fetch 载荷（raw 模式：sseFormat 恒为空串，SSE 语义由前端解析） */
export interface HttpRequestPayload {
  method: string
  url: string
  headers: Record<string, string>
  body?: string
  stream?: boolean
  streamEvent?: string
  /** 字面量类型锁定 raw 模式：任何非空值都会编译报错，防止未来 adapter 误填
     导致宿主与前端双重解析（P1 风险第二条） */
  sseFormat: ''
}

/** 思考类全局配置（插件级 contributes.configuration；各方言适配器自行映射为
    方言参数——openai 写 thinking，无法映射的方言忽略请求侧参数）。
    枚举复用插件类型域，避免与 PluginConfig 两处真源漂移 */
export interface ThinkingOptions {
  thinkingMode: ThinkingMode
  reasoningEffort: ReasoningEffort
}

/** 供应商协议适配器（注册表按 apiStyle 分派） */
export interface ProviderAdapter {
  /** 分派键（openai / anthropic / gemini / custom） */
  apiStyle: ApiStyle
  /** 构建流式对话请求（chat-stream）；options 为思考类全局配置（P3） */
  buildRequest(
    provider: ApiProvider,
    messages: AdapterMessage[],
    streamId: string,
    options?: ThinkingOptions,
  ): HttpRequestPayload
  /** 构建非流式对话请求（chat-complete / 测试连接） */
  buildCompleteRequest(provider: ApiProvider, messages: AdapterMessage[]): HttpRequestPayload
  /** 构建模型列表请求（fetch-models） */
  buildModelsRequest(provider: ApiProvider): HttpRequestPayload
  /** 解析一条 SSE data 载荷（SseBuffer 产出）；无法识别返回 null */
  parseStreamEvent(data: string): StreamEvent | null
  /** 解析非流式对话响应体（chat-complete），返回回复文本 */
  parseCompleteResponse(body: string): string
  /** 解析模型列表响应体（openai/anthropic `data[].id`、gemini `models[].name`）；形状不符抛错 */
  parseModelsResponse(body: string): string[]
}
