/**
 * OpenAI 兼容方言适配器
 *
 * 覆盖 OpenAI / DeepSeek / 通义千问等 /chat/completions 兼容端点：
 * - 请求侧 `stream_options.include_usage` 常开——raw 模式宿主不再透传 usage，
 *   必须依赖流尾 usage 块自解析（P1 风险第一条）
 * - 解析侧提取 `delta.content`（正文）/ `delta.reasoning_content`（DeepSeek
 *   思考模式）/ `usage`（蛇形转驼峰）/ `[DONE]` 终结行
 */
import type { ApiProvider, Usage } from '../types'
import type { HttpRequestPayload, ProviderAdapter, StreamEvent, ThinkingOptions } from './types'
import { effectiveModel, joinUrl, parseDataIdModels, tryParseJson } from './utils'

export const openaiAdapter: ProviderAdapter = {
  apiStyle: 'openai',

  buildRequest(provider, messages, streamId, options) {
    const body: Record<string, unknown> = {
      model: effectiveModel(provider),
      messages: messages.map((m) => ({ role: m.role, content: m.content })),
      stream: true,
      // usage 提取依赖 include_usage 尾块（raw 模式宿主不再透传 usage，P1 风险第一条）
      stream_options: { include_usage: true },
    }
    applyThinking(body, options)
    return {
      method: 'POST',
      url: joinUrl(provider.baseUrl, '/chat/completions'),
      headers: {
        Authorization: `Bearer ${provider.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
      stream: true,
      streamEvent: `ai-chatbox:stream:${streamId}`,
      sseFormat: '',
    }
  },

  buildCompleteRequest(provider, messages) {
    return {
      method: 'POST',
      url: joinUrl(provider.baseUrl, '/chat/completions'),
      headers: {
        Authorization: `Bearer ${provider.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: effectiveModel(provider),
        messages: messages.map((m) => ({ role: m.role, content: m.content })),
        stream: false,
      }),
      sseFormat: '',
    }
  },

  buildModelsRequest(provider) {
    return {
      method: 'GET',
      url: joinUrl(provider.baseUrl, '/models'),
      headers: {
        Authorization: `Bearer ${provider.apiKey}`,
        'Content-Type': 'application/json',
      },
      sseFormat: '',
    }
  },

  parseStreamEvent(data) {
    if (data === '[DONE]') return { done: true }
    const parsed = tryParseJson(data)
    if (!parsed) return null
    const event: StreamEvent = {}
    const delta = parsed.choices?.[0]?.delta
    if (typeof delta?.content === 'string' && delta.content) {
      event.chunk = delta.content
    }
    if (typeof delta?.reasoning_content === 'string' && delta.reasoning_content) {
      event.reasoning = delta.reasoning_content
    }
    if (parsed.usage) {
      event.usage = {
        promptTokens: parsed.usage.prompt_tokens ?? 0,
        completionTokens: parsed.usage.completion_tokens ?? 0,
        totalTokens: parsed.usage.total_tokens ?? 0,
      }
    }
    return event.chunk || event.reasoning || event.usage ? event : null
  },

  parseCompleteResponse(body) {
    const parsed = tryParseJson(body)
    return parsed?.choices?.[0]?.message?.content ?? ''
  },

  parseModelsResponse(body) {
    return parseDataIdModels(body)
  },
}

/** 思考参数映射：仅 thinkingMode ≠ default 时写入 thinking（default 不传参、
    跟随模型自身行为）；reasoning_effort 只在强制开启时有意义，disabled 不携带 */
function applyThinking(body: Record<string, unknown>, options?: ThinkingOptions): void {
  if (!options || options.thinkingMode === 'default') return
  const thinking: Record<string, unknown> = { type: options.thinkingMode }
  if (options.thinkingMode === 'enabled') {
    thinking.reasoning_effort = options.reasoningEffort
  }
  body.thinking = thinking
}
