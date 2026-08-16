/**
 * 协议适配层单测（接缝 1）：各方言请求形状、SSE 事件解析、usage 透传、
 * [DONE] 终结、非流式响应解析、模型列表响应解析、apiStyle 缺失默认
 */
import { describe, it, expect } from 'vitest'
import type { ApiProvider } from '../types'
import { openaiAdapter } from '../adapters/openai'
import { anthropicAdapter } from '../adapters/anthropic'
import { geminiAdapter } from '../adapters/gemini'
import { customAdapter } from '../adapters/custom'
import { getAdapter, parseModelsResponse } from '../adapters/registry'
import type { AdapterMessage } from '../adapters/types'

function provider(overrides: Partial<ApiProvider> = {}): ApiProvider {
  return {
    id: 'p1',
    name: 'DeepSeek',
    apiKey: 'sk-test',
    baseUrl: 'https://api.deepseek.com/v1/',
    apiStyle: 'openai',
    models: ['deepseek-chat', 'deepseek-reasoner'],
    activeModel: 'deepseek-chat',
    ...overrides,
  }
}

function messages(extra: AdapterMessage[] = []): AdapterMessage[] {
  return [
    { role: 'system', content: 'be terse' },
    { role: 'user', content: 'hi' },
    ...extra,
  ]
}

/** 解析请求体 JSON（body 为字符串） */
function bodyOf(request: { body?: string }): any {
  return JSON.parse(request.body ?? '{}')
}

describe('openai adapter', () => {
  it('流式请求形状：url/headers/body/include_usage/streamEvent/raw 模式', () => {
    const req = openaiAdapter.buildRequest(provider(), messages(), 's1')
    expect(req.method).toBe('POST')
    expect(req.url).toBe('https://api.deepseek.com/v1/chat/completions')
    expect(req.headers).toEqual({
      Authorization: 'Bearer sk-test',
      'Content-Type': 'application/json',
    })
    expect(req.stream).toBe(true)
    expect(req.streamEvent).toBe('ai-chatbox:stream:s1')
    // raw 模式：宿主不做 SSE 解析，由前端 SseBuffer + adapter 完成
    expect(req.sseFormat).toBe('')

    const body = bodyOf(req)
    expect(body.model).toBe('deepseek-chat')
    expect(body.stream).toBe(true)
    // usage 提取依赖 include_usage 尾块（raw 模式宿主不再透传 usage）
    expect(body.stream_options).toEqual({ include_usage: true })
    expect(body.messages).toEqual([
      { role: 'system', content: 'be terse' },
      { role: 'user', content: 'hi' },
    ])
  })

  it('对话级 model 覆盖优先于 activeModel', () => {
    const req = openaiAdapter.buildRequest(provider({ model: 'deepseek-reasoner' }), messages(), 's1')
    expect(bodyOf(req).model).toBe('deepseek-reasoner')
  })

  it('thinking 映射：default 不写 thinking 字段（跟随模型自身行为）', () => {
    const req = openaiAdapter.buildRequest(provider(), messages(), 's1', {
      thinkingMode: 'default',
      reasoningEffort: 'high',
    })
    expect(bodyOf(req).thinking).toBeUndefined()
  })

  it('thinking 映射：未传 options 与 default 等价（旧调用方不受影响）', () => {
    const req = openaiAdapter.buildRequest(provider(), messages(), 's1')
    expect(bodyOf(req).thinking).toBeUndefined()
  })

  it('thinking 映射：enabled 写 type=enabled + reasoning_effort', () => {
    const req = openaiAdapter.buildRequest(provider(), messages(), 's1', {
      thinkingMode: 'enabled',
      reasoningEffort: 'max',
    })
    expect(bodyOf(req).thinking).toEqual({ type: 'enabled', reasoning_effort: 'max' })
  })

  it('thinking 映射：disabled 只写 type=disabled，不携带 reasoning_effort（强度仅对开启有意义）', () => {
    const req = openaiAdapter.buildRequest(provider(), messages(), 's1', {
      thinkingMode: 'disabled',
      reasoningEffort: 'high',
    })
    expect(bodyOf(req).thinking).toEqual({ type: 'disabled' })
  })

  it('非流式请求：stream false 且不带 streamEvent', () => {
    const req = openaiAdapter.buildCompleteRequest(provider(), messages())
    expect(req.stream).toBeUndefined()
    expect(req.streamEvent).toBeUndefined()
    expect(bodyOf(req).stream).toBe(false)
    expect(bodyOf(req).stream_options).toBeUndefined()
  })

  it('模型列表请求：GET /models', () => {
    const req = openaiAdapter.buildModelsRequest(provider())
    expect(req.method).toBe('GET')
    expect(req.url).toBe('https://api.deepseek.com/v1/models')
    expect(req.headers.Authorization).toBe('Bearer sk-test')
  })

  it('流解析：delta.content 提取', () => {
    const ev = openaiAdapter.parseStreamEvent(
      JSON.stringify({ choices: [{ delta: { content: '你好' } }] }),
    )
    expect(ev).toEqual({ chunk: '你好' })
  })

  it('流解析：reasoning_content 提取（DeepSeek 思考模式）', () => {
    const ev = openaiAdapter.parseStreamEvent(
      JSON.stringify({ choices: [{ delta: { reasoning_content: '思考中' } }] }),
    )
    expect(ev).toEqual({ reasoning: '思考中' })
  })

  it('流解析：usage 蛇形字段转驼峰透传', () => {
    const ev = openaiAdapter.parseStreamEvent(
      JSON.stringify({
        choices: [],
        usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 },
      }),
    )
    expect(ev?.usage).toEqual({ promptTokens: 12, completionTokens: 5, totalTokens: 17 })
  })

  it('流解析：[DONE] 标记终结', () => {
    expect(openaiAdapter.parseStreamEvent('[DONE]')).toEqual({ done: true })
  })

  it('流解析：非 JSON data 行返回 null', () => {
    expect(openaiAdapter.parseStreamEvent(': ping')).toBeNull()
  })

  it('非流式响应解析：choices[0].message.content', () => {
    const text = openaiAdapter.parseCompleteResponse(
      JSON.stringify({ choices: [{ message: { content: '回复文本' } }] }),
    )
    expect(text).toBe('回复文本')
  })
})

describe('anthropic adapter', () => {
  it('流式请求形状：x-api-key + anthropic-version 头、system 独立、流式 body', () => {
    const req = anthropicAdapter.buildRequest(
      provider({ baseUrl: 'https://api.anthropic.com/v1' }),
      messages(),
      's1',
    )
    expect(req.method).toBe('POST')
    expect(req.url).toBe('https://api.anthropic.com/v1/messages')
    expect(req.headers['x-api-key']).toBe('sk-test')
    expect(req.headers['anthropic-version']).toBe('2023-06-01')
    expect(req.stream).toBe(true)
    expect(req.streamEvent).toBe('ai-chatbox:stream:s1')
    expect(req.sseFormat).toBe('')

    const body = bodyOf(req)
    expect(body.model).toBe('deepseek-chat')
    expect(body.system).toBe('be terse')
    expect(body.max_tokens).toBeGreaterThan(0)
    // system 不能作为消息角色（Anthropic 协议要求独立顶层字段）
    expect(body.messages).toEqual([{ role: 'user', content: 'hi' }])
  })

  it('无 system 消息时不带 system 字段', () => {
    const req = anthropicAdapter.buildRequest(
      provider({ baseUrl: 'https://api.anthropic.com/v1' }),
      [{ role: 'user', content: 'hi' }],
      's1',
    )
    expect(bodyOf(req).system).toBeUndefined()
  })

  it('assistant 角色映射保持 assistant', () => {
    const req = anthropicAdapter.buildRequest(
      provider({ baseUrl: 'https://api.anthropic.com/v1' }),
      [
        { role: 'user', content: 'q' },
        { role: 'assistant', content: 'a' },
      ],
      's1',
    )
    expect(bodyOf(req).messages).toEqual([
      { role: 'user', content: 'q' },
      { role: 'assistant', content: 'a' },
    ])
  })

  it('流解析：content_block_delta.delta.text 提取', () => {
    const ev = anthropicAdapter.parseStreamEvent(
      JSON.stringify({ type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: '你好' } }),
    )
    expect(ev).toEqual({ chunk: '你好' })
  })

  it('流解析：thinking_delta 提取为 reasoning', () => {
    const ev = anthropicAdapter.parseStreamEvent(
      JSON.stringify({ type: 'content_block_delta', index: 0, delta: { type: 'thinking_delta', thinking: '思考' } }),
    )
    expect(ev).toEqual({ reasoning: '思考' })
  })

  it('流解析：message_start 携带输入 usage（部分字段缺省）', () => {
    const ev = anthropicAdapter.parseStreamEvent(
      JSON.stringify({ type: 'message_start', message: { usage: { input_tokens: 25, output_tokens: 1 } } }),
    )
    expect(ev?.usage).toEqual({ promptTokens: 25, completionTokens: 1 })
  })

  it('流解析：message_start 缺省 output_tokens 时不产生 NaN/undefined 字段（真实 API 可能缺失）', () => {
    const ev = anthropicAdapter.parseStreamEvent(
      JSON.stringify({ type: 'message_start', message: { usage: { input_tokens: 25 } } }),
    )
    // 缺省字段不产出（由前端 mergeUsage 兜底，不会出现 NaN/undefined）
    expect(ev?.usage).toEqual({ promptTokens: 25 })
  })

  it('流解析：message_delta 携带输出 usage', () => {
    const ev = anthropicAdapter.parseStreamEvent(
      JSON.stringify({ type: 'message_delta', delta: { stop_reason: 'end_turn' }, usage: { output_tokens: 15 } }),
    )
    expect(ev?.usage).toEqual({ completionTokens: 15 })
  })

  it('流解析：message_stop 标记终结（Anthropic 无 [DONE] 行）', () => {
    expect(anthropicAdapter.parseStreamEvent('{"type":"message_stop"}')).toEqual({ done: true })
  })

  it('非流式响应解析：content 块按 text 类型拼接', () => {
    const text = anthropicAdapter.parseCompleteResponse(
      JSON.stringify({
        content: [
          { type: 'text', text: '第一段' },
          { type: 'tool_use', name: 'x' },
          { type: 'text', text: '第二段' },
        ],
      }),
    )
    expect(text).toBe('第一段第二段')
  })
})

describe('gemini adapter', () => {
  it('流式请求形状：x-goog-api-key + streamGenerateContent?alt=sse', () => {
    const req = geminiAdapter.buildRequest(
      provider({ baseUrl: 'https://generativelanguage.googleapis.com/v1beta' }),
      messages(),
      's1',
    )
    expect(req.method).toBe('POST')
    expect(req.url).toBe(
      'https://generativelanguage.googleapis.com/v1beta/models/deepseek-chat:streamGenerateContent?alt=sse',
    )
    expect(req.headers['x-goog-api-key']).toBe('sk-test')
    expect(req.stream).toBe(true)
    expect(req.sseFormat).toBe('')

    const body = bodyOf(req)
    // role 映射 user/model + system 独立 systemInstruction
    expect(body.contents).toEqual([{ role: 'user', parts: [{ text: 'hi' }] }])
    expect(body.systemInstruction).toEqual({ parts: [{ text: 'be terse' }] })
  })

  it('非流式请求：generateContent 不带 alt=sse', () => {
    const req = geminiAdapter.buildCompleteRequest(
      provider({ baseUrl: 'https://generativelanguage.googleapis.com/v1beta' }),
      messages(),
    )
    expect(req.url).toBe(
      'https://generativelanguage.googleapis.com/v1beta/models/deepseek-chat:generateContent',
    )
    expect(req.stream).toBeUndefined()
  })

  it('流解析：candidates[0].content.parts[0].text 提取', () => {
    const ev = geminiAdapter.parseStreamEvent(
      JSON.stringify({ candidates: [{ content: { parts: [{ text: '你好' }] } }] }),
    )
    expect(ev).toEqual({ chunk: '你好' })
  })

  it('流解析：usageMetadata 透传（每 chunk 累计全量，后到覆盖）', () => {
    const ev = geminiAdapter.parseStreamEvent(
      JSON.stringify({
        candidates: [],
        usageMetadata: { promptTokenCount: 10, candidatesTokenCount: 6, totalTokenCount: 16 },
      }),
    )
    expect(ev?.usage).toEqual({ promptTokens: 10, completionTokens: 6, totalTokens: 16 })
  })

  it('非流式响应解析：parts 文本拼接', () => {
    const text = geminiAdapter.parseCompleteResponse(
      JSON.stringify({ candidates: [{ content: { parts: [{ text: '甲' }, { text: '乙' }] } }] }),
    )
    expect(text).toBe('甲乙')
  })

  it('模型列表响应解析：models[].name 去 models/ 前缀（Gemini 独有形状）', () => {
    const models = geminiAdapter.parseModelsResponse(
      JSON.stringify({
        models: [
          { name: 'models/gemini-2.0-flash', version: '001' },
          { name: 'models/gemini-1.5-pro' },
        ],
      }),
    )
    expect(models).toEqual(['gemini-2.0-flash', 'gemini-1.5-pro'])
  })

  it('模型列表响应解析：非 models 数组形状抛错', () => {
    expect(() => geminiAdapter.parseModelsResponse('{"data":[]}')).toThrow(/bad models response/)
  })
})

describe('custom adapter（逃生舱槽位）', () => {
  it('构建请求抛「未实现」错误（接口预留，不实现 UI）', () => {
    expect(() => customAdapter.buildRequest(provider(), [], 's1')).toThrow(/not implemented/)
    expect(() => customAdapter.buildCompleteRequest(provider(), [])).toThrow(/not implemented/)
    expect(() => customAdapter.buildModelsRequest(provider())).toThrow(/not implemented/)
    expect(() => customAdapter.parseModelsResponse('{}')).toThrow(/not implemented/)
  })

  it('流解析返回 null（不产生任何事件）', () => {
    expect(customAdapter.parseStreamEvent('{"choices":[]}')).toBeNull()
  })
})

describe('注册表', () => {
  it('按 apiStyle 分派', () => {
    expect(getAdapter('openai')).toBe(openaiAdapter)
    expect(getAdapter('anthropic')).toBe(anthropicAdapter)
    expect(getAdapter('gemini')).toBe(geminiAdapter)
    expect(getAdapter('custom')).toBe(customAdapter)
  })

  it('缺失/未知 apiStyle 默认 openai', () => {
    expect(getAdapter(undefined)).toBe(openaiAdapter)
    expect(getAdapter(null)).toBe(openaiAdapter)
    expect(getAdapter('unknown' as any)).toBe(openaiAdapter)
  })

  it('模型列表响应解析：data[].id 抽取（openai 方言）', () => {
    const models = parseModelsResponse(
      'openai',
      JSON.stringify({ data: [{ id: 'a' }, { id: 'b' }, { object: 'list' }] }),
    )
    expect(models).toEqual(['a', 'b'])
  })

  it('模型列表响应解析：anthropic 与 openai 同形状', () => {
    const models = parseModelsResponse(
      'anthropic',
      JSON.stringify({ data: [{ id: 'claude-3-5-sonnet' }], has_more: false }),
    )
    expect(models).toEqual(['claude-3-5-sonnet'])
  })

  it('模型列表响应解析：按 apiStyle 分派（gemini 形状与 openai 不同）', () => {
    const models = parseModelsResponse(
      'gemini',
      JSON.stringify({ models: [{ name: 'models/gemini-2.0-flash' }] }),
    )
    expect(models).toEqual(['gemini-2.0-flash'])
  })

  it('模型列表响应解析：形状不符抛错', () => {
    expect(() => parseModelsResponse('openai', '{"object":"list"}')).toThrow(
      /bad models response/,
    )
    expect(() => parseModelsResponse('openai', 'not json')).toThrow(/bad models response/)
  })
})
