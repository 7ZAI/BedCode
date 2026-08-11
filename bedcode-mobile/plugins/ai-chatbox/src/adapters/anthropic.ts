/**
 * Anthropic Messages 方言适配器
 *
 * 覆盖 Anthropic 官方与 DeepSeek Anthropic 兼容端点：
 * - 请求侧 `x-api-key` + `anthropic-version` 头、system 独立顶层字段
 *   （Anthropic 不允许 system 作为消息角色）、流式 `stream: true`
 * - 解析侧 `content_block_delta.delta.text`（正文）/
 *   `delta.thinking`（扩展思考）/ usage 分事件拆开（message_start 输入、
 *   message_delta 输出，由前端 mergeUsage 合并）/ message_stop 终结
 */
import type { ApiProvider } from '../types'
import type { HttpRequestPayload, ProviderAdapter, StreamEvent } from './types'
import { effectiveModel, joinUrl, parseDataIdModels, tryParseJson } from './utils'

/** Anthropic Messages API 版本头（官方要求固定值） */
const ANTHROPIC_VERSION = '2023-06-01'
/** 对话请求必须显式 max_tokens（缺省会 400），无对话级配置先取固定值 */
const DEFAULT_MAX_TOKENS = 4096

export const anthropicAdapter: ProviderAdapter = {
  apiStyle: 'anthropic',

  // 思考参数映射（P4 接入点）：本期仅 openai 方言消费 options；anthropic
  // 请求侧应在此写入 `thinking: { type: 'enabled', budget_tokens }` 等预算类
  // 参数，避免 thinkingMode=enabled 对本方言静默无操作成为隐性问题
  buildRequest(provider, messages, streamId) {
    const body = buildMessagesBody(provider, messages, true)
    return {
      method: 'POST',
      url: joinUrl(provider.baseUrl, '/messages'),
      headers: {
        'x-api-key': provider.apiKey,
        'anthropic-version': ANTHROPIC_VERSION,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
      stream: true,
      streamEvent: `ai-chatbox:stream:${streamId}`,
      sseFormat: '',
    }
  },

  buildCompleteRequest(provider, messages) {
    const body = buildMessagesBody(provider, messages, false)
    return {
      method: 'POST',
      url: joinUrl(provider.baseUrl, '/messages'),
      headers: {
        'x-api-key': provider.apiKey,
        'anthropic-version': ANTHROPIC_VERSION,
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
        'x-api-key': provider.apiKey,
        'anthropic-version': ANTHROPIC_VERSION,
        'Content-Type': 'application/json',
      },
      sseFormat: '',
    }
  },

  parseStreamEvent(data) {
    const parsed = tryParseJson(data)
    if (!parsed) return null
    switch (parsed.type) {
      case 'content_block_delta': {
        const event: StreamEvent = {}
        const delta = parsed.delta
        if (typeof delta?.text === 'string' && delta.text) {
          event.chunk = delta.text
        }
        if (typeof delta?.thinking === 'string' && delta.thinking) {
          event.reasoning = delta.thinking
        }
        return event.chunk || event.reasoning ? event : null
      }
      case 'message_start': {
        // 输入用量只在首个事件携带（后续 chunk 不再重复）
        if (parsed.message?.usage) {
          return {
            usage: {
              promptTokens: parsed.message.usage.input_tokens,
              completionTokens: parsed.message.usage.output_tokens,
            },
          }
        }
        return null
      }
      case 'message_delta': {
        if (parsed.usage) {
          return { usage: { completionTokens: parsed.usage.output_tokens } }
        }
        return null
      }
      case 'message_stop':
        // Anthropic 无 [DONE] 行，message_stop 即流终结标记
        return { done: true }
      default:
        return null
    }
  },

  parseCompleteResponse(body) {
    const parsed = tryParseJson(body)
    const content = parsed?.content
    if (!Array.isArray(content)) return ''
    return content
      .filter((block: any) => block?.type === 'text')
      .map((block: any) => block?.text ?? '')
      .join('')
  },

  // Anthropic /models 与 OpenAI 同形状（data[].id），直接复用共享解析
  parseModelsResponse(body) {
    return parseDataIdModels(body)
  },
}

/** 组装 Messages 请求体：system 独立顶层字段、其余按 user/assistant 角色映射 */
function buildMessagesBody(
  provider: ApiProvider,
  messages: { role: string; content: string }[],
  stream: boolean,
): Record<string, unknown> {
  const system = messages
    .filter((m) => m.role === 'system')
    .map((m) => m.content)
    .join('\n')
  const body: Record<string, unknown> = {
    model: effectiveModel(provider),
    max_tokens: DEFAULT_MAX_TOKENS,
    messages: messages
      .filter((m) => m.role !== 'system')
      .map((m) => ({ role: m.role === 'assistant' ? 'assistant' : 'user', content: m.content })),
    stream,
  }
  if (system) body.system = system
  return body
}
