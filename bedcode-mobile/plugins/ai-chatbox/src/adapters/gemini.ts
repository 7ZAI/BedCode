/**
 * Gemini 方言适配器
 *
 * `:streamGenerateContent?alt=sse` 流式端点 + `x-goog-api-key` 认证：
 * - 请求侧 contents role 映射 user/model、system 独立为 systemInstruction
 * - 解析侧提取 `candidates[0].content.parts[0].text` 与 `usageMetadata`
 *   （Gemini 流式每 chunk 携带累计全量，后到覆盖即可，无需跨事件合并）
 */
import type { ApiProvider } from '../types'
import type { HttpRequestPayload, ProviderAdapter, StreamEvent } from './types'
import { effectiveModel, joinUrl, tryParseJson } from './utils'

export const geminiAdapter: ProviderAdapter = {
  apiStyle: 'gemini',

  // 思考参数映射（P4 接入点）：本期仅 openai 方言消费 options；gemini
  // 请求侧应在此写入 generationConfig.thinkingConfig.thinkingBudget 等预算类
  // 参数，避免 thinkingMode=enabled 对本方言静默无操作成为隐性问题
  buildRequest(provider, messages, streamId) {
    const body = buildContentsBody(provider, messages)
    const modelPath = `/models/${encodeURIComponent(effectiveModel(provider))}:streamGenerateContent`
    return {
      method: 'POST',
      url: `${joinUrl(provider.baseUrl, modelPath)}?alt=sse`,
      headers: {
        'x-goog-api-key': provider.apiKey,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
      stream: true,
      streamEvent: `ai-chatbox:stream:${streamId}`,
      sseFormat: '',
    }
  },

  buildCompleteRequest(provider, messages) {
    const body = buildContentsBody(provider, messages)
    const modelPath = `/models/${encodeURIComponent(effectiveModel(provider))}:generateContent`
    return {
      method: 'POST',
      url: joinUrl(provider.baseUrl, modelPath),
      headers: {
        'x-goog-api-key': provider.apiKey,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
      sseFormat: '',
    }
  },

  buildModelsRequest(provider) {
    return {
      method: 'GET',
      url: joinUrl(provider.baseUrl, '/models'),
      headers: {
        'x-goog-api-key': provider.apiKey,
        'Content-Type': 'application/json',
      },
      sseFormat: '',
    }
  },

  parseStreamEvent(data) {
    const parsed = tryParseJson(data)
    if (!parsed) return null
    const event: StreamEvent = {}
    const part = parsed.candidates?.[0]?.content?.parts?.[0]
    if (typeof part?.text === 'string' && part.text) {
      event.chunk = part.text
    }
    if (parsed.usageMetadata) {
      event.usage = {
        promptTokens: parsed.usageMetadata.promptTokenCount ?? 0,
        completionTokens: parsed.usageMetadata.candidatesTokenCount ?? 0,
        totalTokens: parsed.usageMetadata.totalTokenCount ?? 0,
      }
    }
    return event.chunk || event.usage ? event : null
  },

  parseCompleteResponse(body) {
    const parsed = tryParseJson(body)
    const parts = parsed?.candidates?.[0]?.content?.parts
    if (!Array.isArray(parts)) return ''
    return parts.map((p: any) => p?.text ?? '').join('')
  },

  // Gemini /models 响应是 `models[].name`（形如 "models/gemini-2.0-flash"），
  // 与 openai/anthropic 的 `data[].id` 形状不同，须按方言单独解析
  parseModelsResponse(body) {
    const parsed = tryParseJson(body)
    if (!Array.isArray(parsed?.models)) {
      throw new Error('bad models response: missing models array')
    }
    return parsed.models
      .map((m: any) => String(m?.name ?? '').replace(/^models\//, ''))
      .filter((name: string) => name.length > 0)
  },
}

/** 组装 contents 请求体：system 独立 systemInstruction、其余映射 user/model */
function buildContentsBody(
  provider: ApiProvider,
  messages: { role: string; content: string }[],
): Record<string, unknown> {
  const systemTexts = messages
    .filter((m) => m.role === 'system')
    .map((m) => m.content)
  const contents = messages
    .filter((m) => m.role !== 'system')
    .map((m) => ({
      role: m.role === 'assistant' ? 'model' : 'user',
      parts: [{ text: m.content }],
    }))
  return {
    contents,
    ...(systemTexts.length
      ? { systemInstruction: { parts: systemTexts.map((t) => ({ text: t })) } }
      : {}),
  }
}
