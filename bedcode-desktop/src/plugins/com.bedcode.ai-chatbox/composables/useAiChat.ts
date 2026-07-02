/**
 * AI 聊天核心逻辑
 *
 * 发送消息、流式接收、对话管理、历史持久化
 * 通过 PluginContext.commands 调用 Rust 后端命令，不再直接调用 openaiClient
 */
import { ref, computed } from 'vue'
import type { ChatMessage, ConversationMeta, ApiProvider } from '../types'
import type { PluginContext } from '../../../plugin/types'

/** 对话管理 composable */
export function useAiChat(context: PluginContext) {
  const conversations = ref<ConversationMeta[]>([])
  const currentConvId = ref<string>('')
  const messages = ref<ChatMessage[]>([])
  const sending = ref(false)
  const streamingContent = ref('')
  const loadingHistory = ref(false)

  /** 当前对话 */
  const currentConversation = computed(() =>
    conversations.value.find(c => c.id === currentConvId.value)
  )

  /** 是否正在流式接收 */
  const isStreaming = computed(() => streamingContent.value !== '')

  /** 生成 UUID */
  function generateId(): string {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
  }

  /** 加载对话列表 — 通过 Rust 后端命令 */
  async function loadConversations(): Promise<void> {
    loadingHistory.value = true
    try {
      const result = await context.commands.execute('ai-chatbox.list-conversations', {})
      if (result && Array.isArray(result)) {
        conversations.value = result
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load conversations:', e)
    } finally {
      loadingHistory.value = false
    }
  }

  /** 加载对话消息 — 通过 Rust 后端命令 */
  async function loadMessages(convId: string): Promise<void> {
    try {
      const result = await context.commands.execute('ai-chatbox.get-messages', { conversationId: convId })
      if (result && Array.isArray(result)) {
        messages.value = result
      } else {
        messages.value = []
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load messages:', e)
      messages.value = []
    }
    currentConvId.value = convId
  }

  /** 保存对话 — 通过 Rust 后端命令 */
  async function saveConversation(conv: ConversationMeta): Promise<void> {
    try {
      await context.commands.execute('ai-chatbox.save-conversation', { conversation: conv })
    } catch (e) {
      console.error('[AI Chatbox] Failed to save conversation:', e)
    }
  }

  /** 保存消息 — 通过 Rust 后端命令 */
  async function saveMessage(conversationId: string, role: string, content: string, timestamp: string): Promise<void> {
    try {
      await context.commands.execute('ai-chatbox.save-message', { conversationId, role, content, timestamp })
    } catch (e) {
      console.error('[AI Chatbox] Failed to save message:', e)
    }
  }

  /** 新建对话 */
  async function newConversation(providerName: string): Promise<void> {
    const conv: ConversationMeta = {
      id: generateId(),
      title: '新对话',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      providerName,
    }
    conversations.value.unshift(conv)
    await saveConversation(conv)
    await loadMessages(conv.id)
  }

  /** 删除对话 — 通过 Rust 后端命令 */
  async function deleteConversation(convId: string): Promise<void> {
    try {
      await context.commands.execute('ai-chatbox.delete-conversation', { conversationId: convId })
    } catch (e) {
      console.error('[AI Chatbox] Failed to delete conversation:', e)
    }
    conversations.value = conversations.value.filter(c => c.id !== convId)
    if (currentConvId.value === convId) {
      currentConvId.value = ''
      messages.value = []
    }
  }

  /** 发送消息 — 通过 Rust 后端流式命令 */
  async function sendMessage(content: string): Promise<void> {
    // 从 storage 读取当前活跃 provider
    const providersStr = await context.storage.get<string>('apiProviders')
    const activeName = await context.storage.get<string>('activeProvider')
    let provider: ApiProvider | undefined
    if (providersStr && activeName) {
      try {
        const parsed = typeof providersStr === 'string' ? JSON.parse(providersStr) : providersStr
        const list = Array.isArray(parsed) ? parsed : []
        provider = list.find((p: ApiProvider) => p.name === activeName)
      } catch { /* ignore */ }
    }
    if (!provider) throw new Error('请先配置 AI 模型')

    // 确保有当前对话
    if (!currentConvId.value) {
      await newConversation(provider.name)
    }

    // 添加用户消息
    const userMsg: ChatMessage = {
      role: 'user',
      content,
      timestamp: new Date().toISOString(),
    }
    messages.value.push(userMsg)

    // 保存用户消息到后端
    await saveMessage(currentConvId.value, 'user', content, userMsg.timestamp)

    // 更新对话标题（首条消息）
    const conv = conversations.value.find(c => c.id === currentConvId.value)
    if (conv && conv.title === '新对话') {
      conv.title = content.slice(0, 30) + (content.length > 30 ? '...' : '')
      conv.updatedAt = new Date().toISOString()
      await saveConversation(conv)
    }

    // 准备 AI 回复占位
    sending.value = true
    streamingContent.value = ''
    const assistantMsg: ChatMessage = {
      role: 'assistant',
      content: '',
      timestamp: new Date().toISOString(),
    }
    messages.value.push(assistantMsg)

    // 构造请求消息（只含 role + content）
    const requestMessages = messages.value
      .filter(m => m.content || m.role === 'assistant')
      .slice(0, -1)
      .map(m => ({ role: m.role, content: m.content }))

    // 生成 streamId 用于监听流式事件
    const streamId = generateId()

    // 监听 Rust 后端流式事件
    const streamDisposable = context.events.on(`ai-chatbox:stream:${streamId}`, (payload: any) => {
      if (payload.chunk) {
        streamingContent.value += payload.chunk
        const last = messages.value[messages.value.length - 1]
        if (last && last.role === 'assistant') {
          last.content = streamingContent.value
        }
      } else if (payload.done) {
        streamDisposable.dispose()
        sending.value = false
        streamingContent.value = ''
        // 保存助手消息到后端
        const finalMsg = messages.value[messages.value.length - 1]
        if (finalMsg && finalMsg.role === 'assistant') {
          saveMessage(currentConvId.value, 'assistant', finalMsg.content, finalMsg.timestamp)
        }
        if (conv) {
          conv.updatedAt = new Date().toISOString()
          saveConversation(conv)
        }
      } else if (payload.error) {
        streamDisposable.dispose()
        sending.value = false
        streamingContent.value = ''
        const last = messages.value[messages.value.length - 1]
        if (last && last.role === 'assistant') {
          last.content = `❌ ${payload.error}`
        }
        saveMessage(currentConvId.value, 'assistant', last?.content || payload.error, assistantMsg.timestamp)
      }
    })

    // 调用 Rust 后端流式命令
    try {
      await context.commands.execute('ai-chatbox.chat-stream', {
        streamId,
        provider,
        messages: requestMessages,
      })
    } catch (e: any) {
      streamDisposable.dispose()
      sending.value = false
      streamingContent.value = ''
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant') {
        last.content = `❌ ${e.message || '请求失败'}`
      }
    }
  }

  /** 停止生成 */
  function stopGeneration(): void {
    sending.value = false
    streamingContent.value = ''
  }

  /** 切换到指定对话 */
  async function switchConversation(convId: string): Promise<void> {
    if (convId === currentConvId.value) return
    await loadMessages(convId)
  }

  return {
    conversations,
    currentConvId,
    messages,
    sending,
    isStreaming,
    loadingHistory,
    currentConversation,
    loadConversations,
    newConversation,
    deleteConversation,
    sendMessage,
    stopGeneration,
    switchConversation,
  }
}
