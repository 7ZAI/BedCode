/**
 * 适配器注册表（ADR-0010 分派点）
 *
 * 按供应商配置的 apiStyle 动态分派请求构建与流解析；缺失/未知值按 openai
 * 处理（与 useAiConfig.normalizeProvider 的旧数据映射语义保持一致）。
 */
import type { ApiProvider, ApiStyle } from '../types'
import type {
  AdapterMessage,
  HttpRequestPayload,
  ProviderAdapter,
  StreamEvent,
  ThinkingOptions,
} from './types'
import { openaiAdapter } from './openai'
import { anthropicAdapter } from './anthropic'
import { geminiAdapter } from './gemini'
import { customAdapter } from './custom'

const ADAPTERS: Record<ApiStyle, ProviderAdapter> = {
  openai: openaiAdapter,
  anthropic: anthropicAdapter,
  gemini: geminiAdapter,
  custom: customAdapter,
}

/** 按 apiStyle 取适配器（缺失/未知默认 openai，与旧数据归一化语义一致） */
export function getAdapter(apiStyle: ApiStyle | undefined | null): ProviderAdapter {
  return (apiStyle && ADAPTERS[apiStyle]) || openaiAdapter
}

/** 构建流式对话请求（chat-stream 载荷）；options 为思考类全局配置（P3），
    由各方言适配器自行映射为方言参数 */
export function buildStreamRequest(
  provider: ApiProvider,
  messages: AdapterMessage[],
  streamId: string,
  options?: ThinkingOptions,
): HttpRequestPayload {
  return getAdapter(provider.apiStyle).buildRequest(provider, messages, streamId, options)
}

/** 构建非流式对话请求（chat-complete / 测试连接载荷） */
export function buildCompleteRequest(
  provider: ApiProvider,
  messages: AdapterMessage[],
): HttpRequestPayload {
  return getAdapter(provider.apiStyle).buildCompleteRequest(provider, messages)
}

/** 构建模型列表请求（fetch-models 载荷） */
export function buildModelsRequest(provider: ApiProvider): HttpRequestPayload {
  return getAdapter(provider.apiStyle).buildModelsRequest(provider)
}

/** 解析一条 SSE data 载荷（SseBuffer 产出） */
export function parseStreamEvent(
  apiStyle: ApiStyle | undefined | null,
  data: string,
): StreamEvent | null {
  return getAdapter(apiStyle).parseStreamEvent(data)
}

/** 解析模型列表响应体（按方言分派：openai/anthropic `data[].id`、gemini `models[].name`；形状不符抛错） */
export function parseModelsResponse(
  apiStyle: ApiStyle | undefined | null,
  body: string,
): string[] {
  return getAdapter(apiStyle).parseModelsResponse(body)
}
