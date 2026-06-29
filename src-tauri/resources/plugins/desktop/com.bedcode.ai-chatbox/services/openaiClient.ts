/**
 * OpenAI 兼容格式 API 客户端
 *
 * 支持流式（SSE）和非流式请求，兼容所有 OpenAI API 格式的模型提供商
 */
import type { ApiProvider, ChatMessage, ProviderPreset } from '../types'

/** 流式回调类型 */
export interface StreamCallbacks {
  onChunk: (text: string) => void
  onDone: () => void
  onError: (error: Error) => void
}

/** 解析 SSE 行，提取 delta content */
function parseSseLine(line: string): string | null {
  const trimmed = line.trim()
  if (!trimmed.startsWith('data: ')) return null
  const data = trimmed.slice(6)
  if (data === '[DONE]') return null
  try {
    const parsed = JSON.parse(data)
    const content = parsed.choices?.[0]?.delta?.content
    return content ?? null
  } catch {
    return null
  }
}

/** 发送聊天请求（流式） */
export async function chatStream(
  provider: ApiProvider,
  messages: ChatMessage[],
  callbacks: StreamCallbacks,
  signal?: AbortSignal,
): Promise<void> {
  const url = `${provider.baseUrl}/chat/completions`
  let response: Response

  try {
    response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${provider.apiKey}`,
      },
      body: JSON.stringify({
        model: provider.model,
        messages: messages.map(m => ({ role: m.role, content: m.content })),
        stream: true,
      }),
      signal,
    })
  } catch (e: any) {
    if (e.name !== 'AbortError') {
      callbacks.onError(new Error(`网络请求失败: ${e.message}`))
    }
    return
  }

  if (!response.ok) {
    let errorMsg = `HTTP ${response.status}`
    try {
      const errBody = await response.json()
      errorMsg = errBody.error?.message || errorMsg
    } catch { /* ignore */ }
    callbacks.onError(new Error(errorMsg))
    return
  }

  const reader = response.body?.getReader()
  if (!reader) {
    callbacks.onError(new Error('无法读取响应流'))
    return
  }

  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      for (const line of lines) {
        const content = parseSseLine(line)
        if (content !== null) {
          callbacks.onChunk(content)
        }
      }
    }

    if (buffer.trim()) {
      const content = parseSseLine(buffer)
      if (content !== null) {
        callbacks.onChunk(content)
      }
    }

    callbacks.onDone()
  } catch (e: any) {
    if (e.name !== 'AbortError') {
      callbacks.onError(new Error(`流读取失败: ${e.message}`))
    }
  }
}

/** 发送聊天请求（非流式，用于提示词优化等短回复场景） */
export async function chat(
  provider: ApiProvider,
  messages: ChatMessage[],
  signal?: AbortSignal,
): Promise<string> {
  const url = `${provider.baseUrl}/chat/completions`
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${provider.apiKey}`,
    },
    body: JSON.stringify({
      model: provider.model,
      messages: messages.map(m => ({ role: m.role, content: m.content })),
      stream: false,
    }),
    signal,
  })

  if (!response.ok) {
    let errorMsg = `HTTP ${response.status}`
    try {
      const errBody = await response.json()
      errorMsg = errBody.error?.message || errorMsg
    } catch { /* ignore */ }
    throw new Error(errorMsg)
  }

  const data = await response.json()
  return data.choices?.[0]?.message?.content || ''
}

/** 预设 Provider 模板 */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  { name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { name: '通义千问', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-turbo' },
  { name: 'Moonshot', baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  { name: '智谱', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4-flash' },
  { name: '硅基流动', baseUrl: 'https://api.siliconflow.cn/v1', model: 'Qwen/Qwen2.5-7B-Instruct' },
]
