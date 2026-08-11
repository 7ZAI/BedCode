/**
 * AI 对话核心逻辑
 *
 * 会话管理（列表/加载/新建/重命名/删除）、流式发送（chunk 累积 + usage）、
 * 停止（本地截断并落盘已接收内容）、重新生成（覆盖旧回复）。
 * 持久化经 `context.commands.execute`（Rust store.rs JSONL）+ 流事件监听。
 */
import { ref, computed } from 'vue'
import type { ChatMessage, ConversationMeta, Usage } from '../types'
import { generateId } from '../types'
import { SseBuffer } from '../adapters/sse'
import { mergeUsage } from '../adapters/usage'
import { buildStreamRequest, getAdapter, parseStreamEvent } from '../adapters/registry'
import { isValidBaseUrl } from '../adapters/utils'
import type { AdapterMessage, StreamEvent } from '../adapters/types'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import type { useAiConfig } from './useAiConfig'

type AiConfig = ReturnType<typeof useAiConfig>

/** 错误分类：识别可提示的常见失败场景（返回宿主 i18n key，未命中返回 null） */
function classifyError(message: string): string | null {
  const m = message.toLowerCase()
  if (/context.*length|maximum context|token.*(limit|exceeded)|context_length/i.test(m)) {
    return 'desktop.plugin.aiChatbox.contextLimitExceeded'
  }
  if (/permission|authorization|access denied|not authorized/i.test(m)) {
    return 'desktop.plugin.aiChatbox.authRevoked'
  }
  return null
}

export function useAiChat(context: PluginContext, config: AiConfig) {
  const conversations = ref<ConversationMeta[]>([])
  const currentConvId = ref('')
  const messages = ref<ChatMessage[]>([])
  const sending = ref(false)
  const streamingContent = ref('')
  /** 流式期间的思考过程（P3 UI 折叠展示；随正文一起截断/覆盖） */
  const streamingReasoning = ref('')
  /** 流终结守卫：adapter [DONE]/message_stop 与宿主 done 事件都可能触发收尾，须幂等 */
  const streamEnded = ref(false)
  const loadingHistory = ref(false)
  /** 最近一次错误（i18n key 或原始文本），组件展示后消费 */
  const lastError = ref('')

  let streamDisposable: { dispose(): void } | null = null

  const currentConversation = computed(() =>
    conversations.value.find(c => c.id === currentConvId.value) || null
  )

  const isStreaming = computed(() => sending.value && streamingContent.value !== '')

  function nowIso(): string {
    return new Date().toISOString()
  }

  // ==================== 会话管理 ====================

  async function loadConversations(): Promise<void> {
    loadingHistory.value = true
    try {
      const result = await context.commands.execute('ai-chatbox.list-conversations', {})
      if (result && Array.isArray(result.conversations)) {
        conversations.value = result.conversations
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load conversations:', e)
    } finally {
      loadingHistory.value = false
    }
  }

  async function loadMessages(convId: string): Promise<void> {
    try {
      const result = await context.commands.execute('ai-chatbox.get-messages', {
        conversationId: convId,
      })
      messages.value = Array.isArray(result?.messages) ? result.messages : []
    } catch (e) {
      console.error('[AI Chatbox] Failed to load messages:', e)
      messages.value = []
    }
    currentConvId.value = convId
  }

  async function saveConversation(conv: ConversationMeta): Promise<void> {
    try {
      await context.commands.execute('ai-chatbox.save-conversation', { conversation: conv })
    } catch (e) {
      console.error('[AI Chatbox] Failed to save conversation:', e)
    }
  }

  async function saveMessage(
    convId: string,
    msg: ChatMessage,
    replaceLastAssistant = false,
  ): Promise<void> {
    try {
      await context.commands.execute('ai-chatbox.save-message', {
        conversationId: convId,
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp,
        model: msg.model || null,
        usage: msg.usage || null,
        replaceLastAssistant,
      })
    } catch (e: any) {
      // fs 写失败（含用户撤销授权）→ 分类提示；其余静默（下次流事件再报）
      const classified = classifyError(String(e?.message || e))
      if (classified) {
        lastError.value = classified
      } else {
        console.error('[AI Chatbox] Failed to save message:', e)
      }
    }
  }

  async function newConversation(): Promise<void> {
    // 流式在途时禁止切换上下文：新建会重置 messages 数组，done 到达时
    // finishStream 会把 assistant 回复落盘到新对话（旧对话缺回复、新对话
    // 混入他人消息），与 switchConversation 的拦截语义保持一致
    if (sending.value) return
    const provider = config.activeProvider.value
    const conv: ConversationMeta = {
      id: generateId(),
      title: 'desktop.plugin.aiChatbox.newConversation',
      createdAt: nowIso(),
      updatedAt: nowIso(),
      providerId: config.activeProviderId.value,
      providerName: provider?.name || '',
      model: config.activeModel.value || provider?.activeModel || '',
      systemPrompt: '',
    }
    conversations.value.unshift(conv)
    await saveConversation(conv)
    await loadMessages(conv.id)
  }

  async function renameConversation(convId: string, title: string): Promise<void> {
    const conv = conversations.value.find(c => c.id === convId)
    if (!conv || !title.trim()) return
    conv.title = title.trim()
    conv.updatedAt = nowIso()
    await saveConversation(conv)
  }

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

  // ==================== 发送 / 流式 ====================

  /** 更新对话 meta 的模型/时间并落盘（发送或流结束时调用） */
  async function touchConversationMeta(): Promise<void> {
    const conv = currentConversation.value
    if (!conv) return
    conv.updatedAt = nowIso()
    conv.model = config.activeModel.value || conv.model
    await saveConversation(conv)
  }

  /** 组装请求消息：systemPrompt（如有） + 全部历史（全量发送，超限由模型报错） */
  function buildRequestMessages(): AdapterMessage[] {
    const result: AdapterMessage[] = []
    const sys = currentConversation.value?.systemPrompt?.trim()
    if (sys) {
      result.push({ role: 'system', content: sys })
    }
    for (const m of messages.value) {
      // 跳过空消息与正在生成的 assistant 占位
      if (!m.content) continue
      result.push({ role: m.role, content: m.content })
    }
    return result
  }

  /** 发送消息；replaceLast 为 true 时（重生成）不新增 user 消息、落盘覆盖旧回复 */
  async function sendMessage(content: string, replaceLast = false): Promise<void> {
    const provider = config.buildRequestProvider()
    if (!provider) {
      lastError.value = 'desktop.plugin.aiChatbox.pleaseConfigure'
      return
    }
    if (sending.value) return

    // 发送前校验（原 Rust 校验前移）：apiKey 非空 + baseUrl 合法
    if (!provider.apiKey.trim()) {
      lastError.value = 'desktop.plugin.aiChatbox.apiKeyRequired'
      return
    }
    if (!isValidBaseUrl(provider.baseUrl)) {
      lastError.value = 'desktop.plugin.aiChatbox.baseUrlInvalid'
      return
    }

    if (!currentConvId.value) {
      await newConversation()
    }
    const conv = currentConversation.value!

    if (!replaceLast) {
      const userMsg: ChatMessage = { role: 'user', content, timestamp: nowIso() }
      messages.value.push(userMsg)
      await saveMessage(conv.id, userMsg)

      // 标题 = 首条消息前 30 字
      if (!conv.title || conv.title === 'desktop.plugin.aiChatbox.newConversation') {
        conv.title = content.slice(0, 30) + (content.length > 30 ? '…' : '')
        await saveConversation(conv)
      }
    }

    const assistantMsg: ChatMessage = {
      role: 'assistant',
      content: '',
      timestamp: nowIso(),
      model: provider.model,
    }
    messages.value.push(assistantMsg)
    sending.value = true
    streamingContent.value = ''
    streamingReasoning.value = ''
    streamEnded.value = false

    const streamId = generateId()
    const requestMessages = buildRequestMessages()
    // 协议适配层构建请求（raw 模式：sseFormat 为空，SSE 语义由前端解析）
    const request = buildStreamRequest(provider, requestMessages, streamId)

    // 每次发送独立的流状态：SSE 缓冲 + usage 累积（adapter 解析结果落地处）
    const sse = new SseBuffer()
    let usageAcc: Usage | undefined

    function applyStreamEvent(ev: StreamEvent): void {
      if (ev.chunk) {
        streamingContent.value += ev.chunk
        const last = messages.value[messages.value.length - 1]
        if (last && last.role === 'assistant') {
          last.content = streamingContent.value
        }
      }
      if (ev.reasoning) {
        streamingReasoning.value += ev.reasoning
        const last = messages.value[messages.value.length - 1]
        if (last && last.role === 'assistant') {
          last.reasoning = streamingReasoning.value
        }
      }
      if (ev.usage) {
        usageAcc = mergeUsage(usageAcc, ev.usage)
      }
      if (ev.done) {
        // adapter 侧终结（openai [DONE] / anthropic message_stop）；宿主 done 仅兜底
        void finishStream(true, undefined, usageAcc, replaceLast)
      }
    }

    streamDisposable = context.events.on(`ai-chatbox:stream:${streamId}`, (payload: any) => {
      if (streamEnded.value) return
      if (typeof payload.chunk === 'string') {
        // 宿主 raw 模式：逐网络 chunk 推原始 SSE 字节，跨 chunk 断行由 SseBuffer 处理
        for (const data of sse.push(payload.chunk)) {
          // 异常服务端可能在 [DONE] 后同一 chunk 还带残余事件：收尾后立即停止消费
          if (streamEnded.value) break
          const ev = parseStreamEvent(provider.apiStyle, data)
          if (ev) applyStreamEvent(ev)
        }
      } else if (payload.error) {
        finishStream(false, payload.error, usageAcc, replaceLast)
      } else if (payload.done) {
        // 宿主 done 兜底：flush 残留缓冲后终结（已终结时幂等跳过）
        for (const data of sse.flush()) {
          if (streamEnded.value) break
          const ev = parseStreamEvent(provider.apiStyle, data)
          if (ev) applyStreamEvent(ev)
        }
        finishStream(true, undefined, usageAcc, replaceLast)
      }
    })

    try {
      await context.commands.execute('ai-chatbox.chat-stream', {
        streamId,
        request,
      })
    } catch (e: any) {
      finishStream(false, String(e?.message || e), usageAcc, replaceLast)
    }
  }

  /** 流结束统一收尾：复位状态（同步，UI 即时响应）+ 落盘 assistant 消息（含 usage）
   *
   * 幂等：adapter [DONE]/message_stop 与宿主 done 事件都会触发，
   * streamEnded 守卫保证只收尾一次（防双重落盘/重复 flush） */
  async function finishStream(
    completed: boolean,
    errorText?: string,
    usage?: Usage,
    replaceAssistantRow = false,
  ): Promise<void> {
    if (streamEnded.value) return
    streamEnded.value = true
    streamDisposable?.dispose()
    streamDisposable = null
    sending.value = false
    streamingContent.value = ''
    streamingReasoning.value = ''

    const last = messages.value[messages.value.length - 1]
    if (last && last.role === 'assistant') {
      if (usage) {
        last.usage = usage
      }
      if (errorText) {
        const classified = classifyError(errorText)
        lastError.value = classified || errorText
      }
      // 流中断/失败也落盘已接收内容；重生成时无条件覆盖旧回复行（防旧回复复现）
      if (completed || last.content.trim() || replaceAssistantRow) {
        await saveMessage(currentConvId.value, last, replaceAssistantRow)
      }
    }
    if (completed) {
      await touchConversationMeta()
    }
  }

  /** 停止生成：本地截断，落盘已接收内容（宿主流任务无法取消，仅停止消费） */
  function stopGeneration(): void {
    if (!sending.value) return
    finishStream(false)
  }

  /** 重新生成：删除最后 assistant 消息（前端 + 文件覆盖），重跑最后一条用户消息 */
  async function regenerate(): Promise<void> {
    if (sending.value) return
    const lastUserIdx = findLastUserIndex()
    if (lastUserIdx === -1) return
    const lastUserContent = messages.value[lastUserIdx].content
    // 移除其后所有 assistant 消息
    messages.value = messages.value.slice(0, lastUserIdx + 1)
    await sendMessage(lastUserContent, true)
  }

  function findLastUserIndex(): number {
    for (let i = messages.value.length - 1; i >= 0; i--) {
      if (messages.value[i].role === 'user') return i
    }
    return -1
  }

  async function switchConversation(convId: string): Promise<void> {
    if (sending.value) return
    if (convId === currentConvId.value) return
    await loadMessages(convId)
  }

  /** 设置对话 system prompt（对话级） */
  async function setSystemPrompt(prompt: string): Promise<void> {
    const conv = currentConversation.value
    if (!conv) return
    conv.systemPrompt = prompt
    await saveConversation(conv)
  }

  return {
    conversations,
    currentConvId,
    currentConversation,
    messages,
    sending,
    isStreaming,
    streamingContent,
    streamingReasoning,
    loadingHistory,
    lastError,
    loadConversations,
    loadMessages,
    newConversation,
    renameConversation,
    deleteConversation,
    sendMessage,
    stopGeneration,
    regenerate,
    switchConversation,
    setSystemPrompt,
  }
}
