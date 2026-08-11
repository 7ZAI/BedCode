/**
 * useAiChat 单测（接缝 3/4）：发送 → 适配层请求 / raw 流解析（chunk/reasoning/usage/done）
 * / 双重终结幂等 / error 分类 / 停止 / 重新生成 / 发送前校验
 */
import { describe, it, expect } from 'vitest'
import { createMockContext, makeProvider } from './mockContext'
import { useAiChat } from '../composables/useAiChat'
import { useAiConfig } from '../composables/useAiConfig'

function setup() {
  const mock = createMockContext()
  const config = useAiConfig(mock.context)
  const chat = useAiChat(mock.context, config)
  return { mock, config, chat }
}

/** 预置供应商并选中 */
async function presetProvider(config: ReturnType<typeof useAiConfig>) {
  await config.addProvider(makeProvider())
  await config.setActiveProvider('p1')
}

/** 构造一条 openai 方言 SSE 事件（data 行 + 空行分隔） */
function sse(payload: unknown): string {
  return `data: ${JSON.stringify(payload)}\n\n`
}

/** 取出最近一次 chat-stream 调用的 streamId 与事件名 */
function streamEventOf(mock: ReturnType<typeof createMockContext>): string {
  const streamCall = mock.calls.filter(c => c.command === 'ai-chatbox.chat-stream').pop()!
  return `ai-chatbox:stream:${streamCall.args.streamId}`
}

/** 模拟一条完整流：正文 chunk + usage 尾块 + [DONE] + 宿主 done 兜底 */
function emitFullStream(mock: ReturnType<typeof createMockContext>, eventName: string, content: string) {
  mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content } }] }) })
  mock.emitStream(eventName, {
    chunk: sse({ choices: [], usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 } }),
  })
  mock.emitStream(eventName, { chunk: 'data: [DONE]\n\n' })
  mock.emitStream(eventName, { done: true })
}

describe('useAiChat', () => {
  it('发送消息：用户消息入列 + chat-stream 携带适配层构建的 raw 请求', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.loadConversations()

    await chat.sendMessage('你好')

    // 自动创建对话
    const convCalls = mock.calls.filter(c => c.command === 'ai-chatbox.save-conversation')
    expect(convCalls.length).toBeGreaterThanOrEqual(1)

    // 用户消息落盘
    const userSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'user')!
    expect(userSave).toBeTruthy()
    expect(userSave.args.content).toBe('你好')

    // chat-stream 载荷：{ streamId, request }（适配层构建，raw 模式）
    const streamCall = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!
    expect(streamCall).toBeTruthy()
    expect(streamCall.args.streamId).toBeTruthy()
    expect(streamCall.args.provider).toBeUndefined()
    const req = streamCall.args.request
    expect(req.method).toBe('POST')
    expect(req.url).toBe('https://api.deepseek.com/v1/chat/completions')
    expect(req.sseFormat).toBe('')
    expect(req.streamEvent).toBe(`ai-chatbox:stream:${streamCall.args.streamId}`)
    const body = JSON.parse(req.body)
    expect(body.model).toBe('deepseek-chat')
    expect(body.messages).toEqual([{ role: 'user', content: '你好' }])
  })

  it('流式 chunk 累积：raw 字节经 SseBuffer 解析到 assistant 消息', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '你' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '好' } }] }) })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.role).toBe('assistant')
    expect(last.content).toBe('你好')
    expect(chat.streamingContent.value).toBe('你好')
  })

  it('流式跨 chunk 断行：SSE 事件被切碎仍完整累积', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    const event = sse({ choices: [{ delta: { content: '跨块' } }] })
    mock.emitStream(eventName, { chunk: event.slice(0, 8) })
    mock.emitStream(eventName, { chunk: event.slice(8) })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.content).toBe('跨块')
  })

  it('思考模式：reasoning_content 累积到消息 reasoning（P1 内存累积，P3 落盘）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { reasoning_content: '思' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { reasoning_content: '考' } }] }) })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.reasoning).toBe('思考')
  })

  it('usage：从流尾 include_usage 块提取（raw 模式宿主 done 不再透传）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '回复' } }] }) })
    mock.emitStream(eventName, {
      chunk: sse({ choices: [], usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 } }),
    })

    // 未终结前不落盘
    expect(chat.sending.value).toBe(true)

    mock.emitStream(eventName, { chunk: 'data: [DONE]\n\n' })

    expect(chat.sending.value).toBe(false)
    expect(chat.streamingContent.value).toBe('')
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave).toBeTruthy()
    expect(assistantSave.args.content).toBe('回复')
    expect(assistantSave.args.usage).toEqual({
      promptTokens: 12,
      completionTokens: 5,
      totalTokens: 17,
    })
  })

  it('双重终结幂等：[DONE] 与宿主 done 事件只落盘一次', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    emitFullStream(mock, streamEventOf(mock), '回复')

    expect(chat.sending.value).toBe(false)
    const assistantSaves = mock.calls.filter(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')
    expect(assistantSaves.length).toBe(1)
  })

  it('[DONE] 后同 chunk 残余事件不再累积（收尾幂等，防落盘与 UI 不一致）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    // 服务端异常：单个网络 chunk 携带 正文 + [DONE] + [DONE] 后的残余正文
    const eventName = streamEventOf(mock)
    const chunk =
      sse({ choices: [{ delta: { content: '回复' } }] }) +
      'data: [DONE]\n\n' +
      sse({ choices: [{ delta: { content: '泄漏' } }] })
    mock.emitStream(eventName, { chunk })

    expect(chat.sending.value).toBe(false)
    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.content).toBe('回复')
    // 残余事件不产生第二次落盘
    const assistantSaves = mock.calls.filter(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')
    expect(assistantSaves.length).toBe(1)
    expect(assistantSaves[0].args.content).toBe('回复')
  })

  it('anthropic 方言：分事件 usage 合并，message_start 缺 output_tokens 无 NaN', async () => {
    const { mock, config, chat } = setup()
    await config.addProvider(makeProvider({ apiStyle: 'anthropic' }))
    await config.setActiveProvider('p1')
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, {
      chunk: sse({ type: 'message_start', message: { usage: { input_tokens: 25 } } }),
    })
    mock.emitStream(eventName, {
      chunk: sse({ type: 'content_block_delta', delta: { type: 'text_delta', text: '你好' } }),
    })
    mock.emitStream(eventName, {
      chunk: sse({ type: 'message_delta', usage: { output_tokens: 15 } }),
    })
    mock.emitStream(eventName, { chunk: sse({ type: 'message_stop' }) })

    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.usage).toEqual({
      promptTokens: 25,
      completionTokens: 15,
      totalTokens: 40,
    })
  })

  it('error 事件：上下文超限关键词 → 分类 i18n key', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { error: 'This model maximum context length is 8192 tokens', done: true })

    expect(chat.lastError.value).toBe('desktop.plugin.aiChatbox.contextLimitExceeded')
  })

  it('命令执行失败：错误分类为授权失效', async () => {
    const mock = createMockContext({
      commands: {
        'ai-chatbox.chat-stream': () => {
          throw new Error('permission denied: path not authorized')
        },
      },
    })
    const config = useAiConfig(mock.context)
    const chat = useAiChat(mock.context, config)
    await config.addProvider(makeProvider())
    await config.setActiveProvider('p1')

    await chat.sendMessage('hi')
    expect(chat.lastError.value).toBe('desktop.plugin.aiChatbox.authRevoked')
  })

  it('发送前校验：apiKey 为空 → apiKeyRequired 提示，不发请求', async () => {
    const { mock, config, chat } = setup()
    await config.addProvider(makeProvider({ apiKey: '' }))
    await config.setActiveProvider('p1')

    await chat.sendMessage('hi')

    expect(chat.lastError.value).toBe('desktop.plugin.aiChatbox.apiKeyRequired')
    expect(mock.calls.some(c => c.command === 'ai-chatbox.chat-stream')).toBe(false)
  })

  it('发送前校验：baseUrl 非法 → baseUrlInvalid 提示，不发请求', async () => {
    const { mock, config, chat } = setup()
    await config.addProvider(makeProvider({ baseUrl: 'not a url' }))
    await config.setActiveProvider('p1')

    await chat.sendMessage('hi')

    expect(chat.lastError.value).toBe('desktop.plugin.aiChatbox.baseUrlInvalid')
    expect(mock.calls.some(c => c.command === 'ai-chatbox.chat-stream')).toBe(false)
  })

  it('停止生成：保存已接收内容并复位', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '部分内容' } }] }) })

    chat.stopGeneration()

    expect(chat.sending.value).toBe(false)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('部分内容')
  })

  it('停止后到达的迟到事件不再累积（幂等收尾）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '部分' } }] }) })
    chat.stopGeneration()
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '迟到' } }] }) })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.content).toBe('部分')
  })

  it('重新生成：截断最后 assistant + 覆盖落盘 + 重发最后用户消息', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('问题一')
    emitFullStream(mock, streamEventOf(mock), '答案一')

    await chat.regenerate()

    // 最后一条消息是新 assistant 占位（旧回复已被截断）
    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.role).toBe('assistant')
    expect(last.content).toBe('')

    // 落盘走 replaceLastAssistant（覆盖旧回复行）
    const streamCalls = mock.calls.filter(c => c.command === 'ai-chatbox.chat-stream')
    expect(streamCalls.length).toBe(2)
    const body = JSON.parse(streamCalls[1].args.request.body)
    expect(body.messages[0]).toEqual({ role: 'user', content: '问题一' })
  })

  it('对话管理：新建/重命名/删除', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.loadConversations()
    await chat.newConversation()

    expect(chat.currentConvId.value).toBeTruthy()
    const convId = chat.currentConvId.value

    await chat.renameConversation(convId, '新标题')
    expect(chat.currentConversation.value?.title).toBe('新标题')

    await chat.deleteConversation(convId)
    expect(chat.conversations.value.find(c => c.id === convId)).toBeUndefined()
    expect(chat.currentConvId.value).toBe('')
  })
})
