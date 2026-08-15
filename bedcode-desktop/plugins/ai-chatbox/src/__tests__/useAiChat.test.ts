/**
 * useAiChat 单测（接缝 3/4）：发送 → 适配层请求 / raw 流解析（chunk/reasoning/usage/done）
 * / 双重终结幂等 / error 分类 / 停止 / 重新生成 / 发送前校验
 */
import { describe, it, expect } from 'vitest'
import { ref } from 'vue'
import { createMockContext, makeProvider } from './mockContext'
import { useAiChat } from '../composables/useAiChat'
import { useAiConfig } from '../composables/useAiConfig'
import type { PluginConfig } from '../types'

/** rAF 调度器 stub：手动触发帧回调，验证节流语义（接缝 3） */
function makeRafStub() {
  let nextId = 1
  const pending: { id: number; cb: FrameRequestCallback }[] = []
  const cancelled: number[] = []
  return {
    scheduler: {
      requestAnimationFrame(cb: FrameRequestCallback) {
        pending.push({ id: nextId, cb })
        return nextId++
      },
      cancelAnimationFrame(id: number) {
        cancelled.push(id)
        const i = pending.findIndex(p => p.id === id)
        if (i !== -1) pending.splice(i, 1)
      },
    },
    pending,
    cancelled,
    /** 触发下一帧回调（模拟浏览器渲染帧） */
    fire() {
      pending.shift()?.cb(0)
    },
  }
}

type RafStub = ReturnType<typeof makeRafStub>

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

  it('思考模式：reasoning_content 累积到消息 reasoning，流结束后随正文落盘', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { reasoning_content: '思' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { reasoning_content: '考' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '正文' } }] }) })
    mock.emitStream(eventName, { chunk: 'data: [DONE]\n\n' })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.reasoning).toBe('思考')
    // save-message 携带 reasoning（P3 落盘：历史重开可见）
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.reasoning).toBe('思考')
    expect(assistantSave.args.content).toBe('正文')
  })

  it('重新生成：旧 reasoning 与新正文一并覆盖（replaceLast 覆盖语义）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('问题')

    // 第一轮：带思考的完整流
    const firstEvent = streamEventOf(mock)
    mock.emitStream(firstEvent, {
      chunk: sse({ choices: [{ delta: { content: '旧正文', reasoning_content: '旧思考' } }] }),
    })
    mock.emitStream(firstEvent, { chunk: 'data: [DONE]\n\n' })

    await chat.regenerate()
    // 第二轮：新思考 + 新正文
    const secondEvent = streamEventOf(mock)
    mock.emitStream(secondEvent, {
      chunk: sse({ choices: [{ delta: { content: '新正文', reasoning_content: '新思考' } }] }),
    })
    mock.emitStream(secondEvent, { chunk: 'data: [DONE]\n\n' })

    const assistantSaves = mock.calls.filter(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')
    expect(assistantSaves.length).toBe(2)
    // 第二轮落盘：replaceLast 覆盖 + 新正文 + 新思考（旧思考不残留）
    expect(assistantSaves[1].args.replaceLastAssistant).toBe(true)
    expect(assistantSaves[1].args.content).toBe('新正文')
    expect(assistantSaves[1].args.reasoning).toBe('新思考')
  })

  it('插件配置：thinkingMode=enabled + effort 透传到 chat-stream 请求体', async () => {
    const mock = createMockContext()
    const config = useAiConfig(mock.context)
    const pluginConfig = ref<PluginConfig>({ thinkingMode: 'enabled', reasoningEffort: 'max', showReasoning: true, codeLineHeight: 1.6, codeFontSize: 13, codeTheme: 'auto' })
    const chat = useAiChat(mock.context, config, undefined, pluginConfig)
    await config.addProvider(makeProvider())
    await config.setActiveProvider('p1')

    await chat.sendMessage('hi')

    const streamCall = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!
    const body = JSON.parse(streamCall.args.request.body)
    expect(body.thinking).toEqual({ type: 'enabled', reasoning_effort: 'max' })
  })

  it('插件配置：thinkingMode=default 不写 thinking 字段（跟随模型）', async () => {
    const mock = createMockContext()
    const config = useAiConfig(mock.context)
    const pluginConfig = ref<PluginConfig>({ thinkingMode: 'default', reasoningEffort: 'high', showReasoning: true, codeLineHeight: 1.6, codeFontSize: 13, codeTheme: 'auto' })
    const chat = useAiChat(mock.context, config, undefined, pluginConfig)
    await config.addProvider(makeProvider())
    await config.setActiveProvider('p1')

    await chat.sendMessage('hi')

    const streamCall = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!
    expect(JSON.parse(streamCall.args.request.body).thinking).toBeUndefined()
  })

  it('停止生成：只有思考内容无正文时也落盘（不丢已接收的推理）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { reasoning_content: '部分思考' } }] }) })
    chat.stopGeneration()

    expect(chat.sending.value).toBe(false)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('')
    expect(assistantSave.args.reasoning).toBe('部分思考')
  })

  it('推理-only 阶段 isStreaming=true（思考期思考块展开/停止按钮联动，可中断）', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    // 正文未到达、仅 reasoning chunk（deepseek-reasoner 思考期典型形态）
    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { reasoning_content: '思考中' } }] }) })

    expect(chat.isStreaming.value).toBe(true)
    expect(chat.streamingContent.value).toBe('')
    expect(chat.streamingReasoning.value).toBe('思考中')
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

describe('rAF 节流 flush（接缝 3，P2 渲染管线）', () => {
  /** 预置供应商 + 注入 stub 调度器 */
  async function setupWithScheduler() {
    const mock = createMockContext()
    const config = useAiConfig(mock.context)
    const stub: RafStub = makeRafStub()
    const chat = useAiChat(mock.context, config, stub.scheduler)
    await config.addProvider(makeProvider())
    await config.setActiveProvider('p1')
    await chat.sendMessage('hi')
    return { mock, config, chat, stub }
  }

  it('多 chunk 合并到一帧：rAF 回调前不更新，回调后批量写回', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '你' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '好' } }] }) })

    // 一帧内多个 chunk 只挂起一次 flush
    expect(stub.pending.length).toBe(1)
    expect(chat.streamingContent.value).toBe('')
    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.content).toBe('')

    stub.fire()
    expect(chat.streamingContent.value).toBe('你好')
    expect(chat.messages.value[chat.messages.value.length - 1].content).toBe('你好')
  })

  it('帧回调执行后再来 chunk：重新挂起新帧', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '你' } }] }) })
    stub.fire()
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '好' } }] }) })

    expect(stub.pending.length).toBe(1)
    expect(chat.streamingContent.value).toBe('你')
    stub.fire()
    expect(chat.streamingContent.value).toBe('你好')
  })

  it('reasoning 与正文同帧批量写回', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, {
      chunk: sse({ choices: [{ delta: { content: '回', reasoning_content: '思' } }] }),
    })
    expect(chat.streamingReasoning.value).toBe('')

    stub.fire()
    expect(chat.streamingReasoning.value).toBe('思')
    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.reasoning).toBe('思')
  })

  it('done 立即 flush：取消待决 rAF，终态含最后一批 chunk 与 usage', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '部分' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '回复' } }] }) })
    mock.emitStream(eventName, {
      chunk: sse({ choices: [], usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 } }),
    })
    // 不触发帧回调，直接 [DONE] 终结：待决 rAF 应被取消并立即 flush
    mock.emitStream(eventName, { chunk: 'data: [DONE]\n\n' })

    expect(stub.cancelled.length).toBe(1)
    expect(stub.pending.length).toBe(0)
    expect(chat.sending.value).toBe(false)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('部分回复')
    expect(assistantSave.args.usage).toEqual({
      promptTokens: 12,
      completionTokens: 5,
      totalTokens: 17,
    })
  })

  it('停止生成：残留缓冲立即 flush 后落盘', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '部分' } }] }) })
    chat.stopGeneration()

    expect(stub.cancelled.length).toBe(1)
    expect(chat.sending.value).toBe(false)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('部分')
  })

  it('error 终态：缓冲内容同样 flush 后落盘', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '部分' } }] }) })
    mock.emitStream(eventName, { error: 'boom', done: true })

    expect(stub.cancelled.length).toBe(1)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('部分')
  })

  it('迟到事件在收尾后不再累积（取消后的帧回调不会再触发）', async () => {
    const { mock, chat, stub } = await setupWithScheduler()
    const eventName = streamEventOf(mock)

    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '部分' } }] }) })
    chat.stopGeneration()
    // 取消后帧回调已从队列移除，即使浏览器仍触发也不会写入
    stub.fire()
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '迟到' } }] }) })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.content).toBe('部分')
  })

  it('命令抛错：取消待决 rAF 并 flush 缓冲后收尾（catch → finishStream(false)）', async () => {
    // 模拟"命令启动后、连接出错前服务端已推字节"：先派发一个 chunk 再抛错
    const mock = createMockContext({
      commands: {
        'ai-chatbox.chat-stream': (args: any) => {
          mock.listeners[`ai-chatbox:stream:${args.streamId}`]?.({
            chunk: sse({ choices: [{ delta: { content: '部分' } }] }),
          })
          throw new Error('connection reset')
        },
      },
    })
    const config = useAiConfig(mock.context)
    const stub: RafStub = makeRafStub()
    const chat = useAiChat(mock.context, config, stub.scheduler)
    await config.addProvider(makeProvider())
    await config.setActiveProvider('p1')

    await chat.sendMessage('hi')

    // 抛错前 chunk 已挂起 rAF：收尾路径必须取消它并立即写回缓冲
    expect(stub.cancelled.length).toBe(1)
    expect(stub.pending.length).toBe(0)
    expect(chat.sending.value).toBe(false)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('部分')
  })

  it('无 rAF 环境（默认不注入调度器）：退化为同步写回，行为与 P1 一致', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const eventName = streamEventOf(mock)
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '你' } }] }) })
    mock.emitStream(eventName, { chunk: sse({ choices: [{ delta: { content: '好' } }] }) })

    // 无需触发任何帧回调，chunk 立即生效
    expect(chat.streamingContent.value).toBe('你好')
  })
})
