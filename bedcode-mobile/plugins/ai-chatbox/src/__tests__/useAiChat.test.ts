/**
 * useAiChat 单测（接缝 4）：发送 → 命令参数 / chunk 累积 / done 保存（含 usage）/
 * error 分类 / 停止 / 重新生成
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

describe('useAiChat', () => {
  it('发送消息：用户消息入列 + chat-stream 命令携带 provider/messages', async () => {
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

    // chat-stream 参数
    const streamCall = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!
    expect(streamCall).toBeTruthy()
    expect(streamCall.args.streamId).toBeTruthy()
    expect(streamCall.args.provider.id).toBe('p1')
    expect(streamCall.args.messages).toEqual([
      { role: 'user', content: '你好' },
    ])
  })

  it('流式 chunk 累积到 assistant 消息', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const streamEvent = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!.args.streamId
    const eventName = `ai-chatbox:stream:${streamEvent}`

    mock.emitStream(eventName, { chunk: '你', done: false })
    mock.emitStream(eventName, { chunk: '好', done: false })

    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.role).toBe('assistant')
    expect(last.content).toBe('你好')
    expect(chat.streamingContent.value).toBe('你好')
  })

  it('done 事件：assistant 消息落盘（含 usage）+ 状态复位', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const streamEvent = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!.args.streamId
    const eventName = `ai-chatbox:stream:${streamEvent}`

    mock.emitStream(eventName, { chunk: '回复', done: false })
    mock.emitStream(eventName, {
      done: true,
      usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 },
    })

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

  it('error 事件：上下文超限关键词 → 分类 i18n key', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const streamEvent = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!.args.streamId
    const eventName = `ai-chatbox:stream:${streamEvent}`

    mock.emitStream(eventName, { error: 'This model maximum context length is 8192 tokens', done: true })

    expect(chat.lastError.value).toBe('mobile.plugin.aiChatbox.contextLimitExceeded')
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
    expect(chat.lastError.value).toBe('mobile.plugin.aiChatbox.authRevoked')
  })

  it('停止生成：保存已接收内容并复位', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('hi')

    const streamEvent = mock.calls.find(c => c.command === 'ai-chatbox.chat-stream')!.args.streamId
    const eventName = `ai-chatbox:stream:${streamEvent}`
    mock.emitStream(eventName, { chunk: '部分内容', done: false })

    chat.stopGeneration()

    expect(chat.sending.value).toBe(false)
    const assistantSave = mock.calls.find(c =>
      c.command === 'ai-chatbox.save-message' && c.args.role === 'assistant')!
    expect(assistantSave.args.content).toBe('部分内容')
  })

  it('重新生成：截断最后 assistant + 覆盖落盘 + 重发最后用户消息', async () => {
    const { mock, config, chat } = setup()
    await presetProvider(config)
    await chat.sendMessage('问题一')
    const streamEvent = mock.calls.filter(c => c.command === 'ai-chatbox.chat-stream').pop()!.args.streamId
    mock.emitStream(`ai-chatbox:stream:${streamEvent}`, { chunk: '答案一', done: false })
    mock.emitStream(`ai-chatbox:stream:${streamEvent}`, { done: true })

    await chat.regenerate()

    // 最后一条消息是新 assistant 占位（旧回复已被截断）
    const last = chat.messages.value[chat.messages.value.length - 1]
    expect(last.role).toBe('assistant')
    expect(last.content).toBe('')

    // 落盘走 replaceLastAssistant（覆盖旧回复行）
    const streamCalls = mock.calls.filter(c => c.command === 'ai-chatbox.chat-stream')
    expect(streamCalls.length).toBe(2)
    expect(streamCalls[1].args.messages[0]).toEqual({ role: 'user', content: '问题一' })
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
