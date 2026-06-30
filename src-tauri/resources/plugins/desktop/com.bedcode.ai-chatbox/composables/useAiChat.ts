/**
 * AI 聊天核心逻辑
 *
 * 发送消息、流式接收、对话管理、历史持久化
 */
import { ref, computed } from 'vue'
import type { ChatMessage, ConversationMeta, ApiProvider } from '../types'
import { chatStream } from '../services/openaiClient'

/** 对话管理 composable */
export function useAiChat(
  storageGet: (key: string) => Promise<any>,
  storageSet: (key: string, value: any) => Promise<void>,
  storageDelete: (key: string) => Promise<void>,
  getActiveProvider: () => ApiProvider | undefined,
) {
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

  /** 加载对话列表 */
  async function loadConversations(): Promise<void> {
    loadingHistory.value = true
    try {
      const saved = await storageGet('conversations')
      if (saved) {
        const parsed = typeof saved === 'string' ? JSON.parse(saved) : saved
        conversations.value = Array.isArray(parsed) ? parsed : []
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load conversations:', e)
    } finally {
      loadingHistory.value = false
    }
  }

  /** 加载对话消息 */
  async function loadMessages(convId: string): Promise<void> {
    try {
      const saved = await storageGet(`conv:${convId}`)
      if (saved) {
        const parsed = typeof saved === 'string' ? JSON.parse(saved) : saved
        messages.value = Array.isArray(parsed) ? parsed : []
      } else {
        messages.value = []
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load messages:', e)
      messages.value = []
    }
    currentConvId.value = convId
  }

  /** 保存对话列表 */
  async function saveConversations(): Promise<void> {
    try {
      await storageSet('conversations', JSON.stringify(conversations.value))
    } catch (e) {
      console.error('[AI Chatbox] Failed to save conversations:', e)
    }
  }

  /** 保存当前对话消息 */
  async function saveMessages(): Promise<void> {
    if (!currentConvId.value) return
    try {
      await storageSet(`conv:${currentConvId.value}`, JSON.stringify(messages.value))
    } catch (e) {
      console.error('[AI Chatbox] Failed to save messages:', e)
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
    await saveConversations()
    await loadMessages(conv.id)
  }

  /** 删除对话 */
  async function deleteConversation(convId: string): Promise<void> {
    try {
      await storageDelete(`conv:${convId}`)
    } catch (e) {
      console.error('[AI Chatbox] Failed to delete conversation:', e)
    }
    conversations.value = conversations.value.filter(c => c.id !== convId)
    await saveConversations()
    if (currentConvId.value === convId) {
      currentConvId.value = ''
      messages.value = []
    }
  }

  /** 发送消息 */
  async function sendMessage(content: string): Promise<void> {
    const provider = getActiveProvider()
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

    // 更新对话标题（首条消息）
    const conv = conversations.value.find(c => c.id === currentConvId.value)
    if (conv && conv.title === '新对话') {
      conv.title = content.slice(0, 30) + (content.length > 30 ? '...' : '')
      conv.updatedAt = new Date().toISOString()
      await saveConversations()
    }

    await saveMessages()

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

    // 流式调用
    await chatStream(
      provider,
      requestMessages,
      {
        onChunk: (text) => {
          streamingContent.value += text
          const last = messages.value[messages.value.length - 1]
          if (last && last.role === 'assistant') {
            last.content = streamingContent.value
          }
        },
        onDone: async () => {
          sending.value = false
          streamingContent.value = ''
          await saveMessages()
          if (conv) {
            conv.updatedAt = new Date().toISOString()
            await saveConversations()
          }
        },
        onError: async (error) => {
          sending.value = false
          streamingContent.value = ''
          const last = messages.value[messages.value.length - 1]
          if (last && last.role === 'assistant') {
            last.content = `❌ ${error.message}`
          }
          await saveMessages()
        },
      },
    )
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
