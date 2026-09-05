/**
 * AI 对话核心逻辑
 *
 * 会话管理（列表/加载/新建/重命名/删除）、流式发送（chunk 累积 + usage）、
 * 停止（本地截断并落盘已接收内容）、重新生成（覆盖旧回复）。
 * 持久化经 `context.commands.execute`（Rust store.rs JSONL）+ 流事件监听。
 * 协议差异收敛在 `src/adapters/`（ADR-0010）：发送载荷为适配层构建的 raw
 * 请求（{ streamId, request }），宿主 raw 模式逐字节推流、前端自解析 SSE。
 */
import { ref, computed, type Ref } from 'vue'
import type { ApiProvider, ChatMessage, ConversationMeta, PluginConfig, Usage } from '../types'
import { DEFAULT_PLUGIN_CONFIG, generateId } from '../types'
import { SseBuffer } from '../adapters/sse'
import { mergeUsage } from '../adapters/usage'
import { buildStreamRequest, parseStreamEvent } from '../adapters/registry'
import { isValidBaseUrl } from '../adapters/utils'
import type { AdapterMessage, StreamEvent, ThinkingOptions } from '../adapters/types'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import type { useAiConfig } from './useAiConfig'

type AiConfig = ReturnType<typeof useAiConfig>

/** rAF 调度器抽象：vitest 等无 rAF 环境可注入 fake 实现（接缝 3） */
export interface FrameScheduler {
  requestAnimationFrame(callback: FrameRequestCallback): number
  cancelAnimationFrame(handle: number): void
}

/** 错误分类：识别可提示的常见失败场景（返回宿主 i18n key，未命中返回 null） */
function classifyError(message: string): string | null {
  const m = message.toLowerCase()
  if (/context.*length|maximum context|token.*(limit|exceeded)|context_length/i.test(m)) {
    return 'mobile.plugin.aiChatbox.contextLimitExceeded'
  }
  if (/permission|authorization|access denied|not authorized/i.test(m)) {
    return 'mobile.plugin.aiChatbox.authRevoked'
  }
  return null
}

// ==================== 限流（429 等）自动重试（指数退避） ====================

/** 输入区上方滑出条的限流重试状态（null = 无进行中的重试等待） */
export interface RateLimitRetryState {
  /** 即将进行的重试序号（1-based） */
  attempt: number
  maxRetries: number
  /** 下次重试剩余秒数（500ms 间隔从绝对截止时间换算，避免调度误差漂移） */
  countdownSec: number
  /** 限流 HTTP 状态码（从宿主错误文本解析，未识别为 null） */
  status: number | null
}

/** 从宿主错误文本解析 HTTP 状态码（流式非 2xx 统一为 "API error {status}: {body}"） */
function parseErrorStatus(message: string): number | null {
  const m = /API error (\d{3}):/.exec(message)
  return m ? Number(m[1]) : null
}

/** 限流判定：429 + 常见过载码（503/529，与主流客户端实践对齐），或错误文本明确提及 rate limit */
function isRateLimitError(message: string): boolean {
  const status = parseErrorStatus(message)
  if (status === 429 || status === 503 || status === 529) return true
  return /rate.?limit|too many requests/i.test(message)
}

export function useAiChat(
  context: PluginContext,
  config: AiConfig,
  scheduler?: FrameScheduler,
  /** 插件级全局配置（thinkingMode/reasoningEffort）；未注入时按默认值构建请求 */
  pluginConfig?: Ref<PluginConfig>,
) {
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

  // ==================== 限流重试状态（滑出条 + 退避定时器） ====================

  /** 限流重试等待态（null = 无进行中的重试） */
  const rateLimitRetry = ref<RateLimitRetryState | null>(null)
  /** 已执行的限流重试次数（单次发送内累计，sendMessage 起始复位） */
  let rateLimitAttempt = 0
  /** 限流退避定时器（重试触发 setTimeout + 倒计时刷新 setInterval） */
  let retryFireTimer: ReturnType<typeof setTimeout> | null = null
  let retryCountdownTimer: ReturnType<typeof setInterval> | null = null

  function clearRateLimitTimers(): void {
    if (retryFireTimer !== null) {
      clearTimeout(retryFireTimer)
      retryFireTimer = null
    }
    if (retryCountdownTimer !== null) {
      clearInterval(retryCountdownTimer)
      retryCountdownTimer = null
    }
  }

  // ==================== rAF 节流 flush（P2 渲染管线） ====================
  // chunk 先累积到缓冲，rAF 回调批量写回 streamingContent（每帧至多一次全量
  // 渲染）；done/停止/失败时取消待决 rAF 并立即写回终态。无 rAF 环境（vitest
  // node）退化为同步写回，保持既有行为。
  const raf = scheduler?.requestAnimationFrame ?? globalThis.requestAnimationFrame
  const caf = scheduler?.cancelAnimationFrame ?? globalThis.cancelAnimationFrame
  const hasRaf = typeof raf === 'function'

  /** 待 rAF 写回的流式内容缓冲（单流在途，composable 级即可） */
  let pendingContent = ''
  let pendingReasoning = ''
  /** 待决 rAF 句柄（null 表示本帧无挂起 flush） */
  let rafId: number | null = null

  /** rAF 回调：批量把缓冲写回 streamingContent 与最后一条 assistant 消息 */
  function flushStreamingState(): void {
    rafId = null
    if (!pendingContent && !pendingReasoning) return
    streamingContent.value += pendingContent
    streamingReasoning.value += pendingReasoning
    const last = messages.value[messages.value.length - 1]
    if (last && last.role === 'assistant') {
      last.content = streamingContent.value
      last.reasoning = streamingReasoning.value
    }
    pendingContent = ''
    pendingReasoning = ''
  }

  /** 挂起本帧 flush（每帧至多一次）；无 rAF 环境直接同步写回 */
  function scheduleFlush(): void {
    if (hasRaf) {
      if (rafId !== null) return
      rafId = raf!(() => {
        flushStreamingState()
      })
    } else {
      flushStreamingState()
    }
  }

  /** 立即写回缓冲（终态路径：done/停止/失败时取消待决 rAF 后调用） */
  function flushStreamingNow(): void {
    if (hasRaf && rafId !== null) {
      caf?.(rafId)
      rafId = null
    }
    flushStreamingState()
  }

  let streamDisposable: { dispose(): void } | null = null

  const currentConversation = computed(() =>
    conversations.value.find(c => c.id === currentConvId.value) || null
  )

  // 推理-only 阶段（deepseek-reasoner 思考期可达数十秒）正文为空但思考流已在写入：
  // isStreaming 必须覆盖 reasoning，否则思考块不展开、停止按钮消失、输入框被
  // disabled，用户无法中断（P3 审查 Major）
  const isStreaming = computed(
    () => sending.value && (streamingContent.value !== '' || streamingReasoning.value !== ''),
  )

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
        // 思考过程随正文一起落盘（P3）；重生成 replaceLast 时一并覆盖
        reasoning: msg.reasoning || null,
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
      title: 'mobile.plugin.aiChatbox.newConversation',
      createdAt: nowIso(),
      updatedAt: nowIso(),
      providerId: config.activeProviderId.value,
      providerName: provider?.name || '',
      model: config.activeModel.value || provider?.activeModel || '',
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

  /** 更新对话 meta 的供应商/模型/时间并落盘（发送或流结束时调用） */
  async function touchConversationMeta(): Promise<void> {
    const conv = currentConversation.value
    if (!conv) return
    conv.updatedAt = nowIso()
    // 会话跟随当前选择：多供应商模型混选时 meta 记录实际使用的供应商（后端索引据此展示）
    const provider = config.activeProvider.value
    if (provider) {
      conv.providerId = provider.id
      conv.providerName = provider.name
    }
    conv.model = config.activeModel.value || conv.model
    await saveConversation(conv)
  }

  /** 组装请求消息：全部历史（全量发送，超限由模型报错） */
  function buildRequestMessages(): AdapterMessage[] {
    const result: AdapterMessage[] = []
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
      lastError.value = 'mobile.plugin.aiChatbox.pleaseConfigure'
      return
    }
    if (sending.value) return

    // 发送前校验（原 Rust 校验前移）：apiKey 非空 + baseUrl 合法
    if (!provider.apiKey.trim()) {
      lastError.value = 'mobile.plugin.aiChatbox.apiKeyRequired'
      return
    }
    if (!isValidBaseUrl(provider.baseUrl)) {
      lastError.value = 'mobile.plugin.aiChatbox.baseUrlInvalid'
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
      if (!conv.title || conv.title === 'mobile.plugin.aiChatbox.newConversation') {
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
    pendingContent = ''
    pendingReasoning = ''
    rateLimitAttempt = 0

    const requestMessages = buildRequestMessages()

    // 嵌套函数声明内使用 provider：TS 对提升声明不做 const 窄化保留，
    // 以初始化点已完成窄化的别名传递非空引用
    const activeProvider: ApiProvider = provider

    /** 限流重试参数（每次调度时重新读取，跟随插件配置实时修改） */
    function rateLimitParams() {
      const pc = pluginConfig?.value ?? DEFAULT_PLUGIN_CONFIG
      return {
        maxRetries: pc.rateLimitMaxRetries,
        initialDelayMs: pc.rateLimitInitialDelayMs,
        maxDelayMs: pc.rateLimitMaxDelayMs,
      }
    }

    /** 本轮尝试是否未收到任何内容（流式开始前的限流才可安全重发，中途限流重发会重复整段请求） */
    function nothingReceived(): boolean {
      return (
        !streamingContent.value &&
        !streamingReasoning.value &&
        !pendingContent &&
        !pendingReasoning
      )
    }

    /** 发起一次流式尝试：新 streamId + 独立监听 + 独立 SSE 缓冲（限流重试时整体重建） */
    function startStream(): void {
      const streamId = generateId()
      const sse = new SseBuffer()
      let usageAcc: Usage | undefined

      // 协议适配层构建请求（raw 模式：sseFormat 为空，SSE 语义由前端解析）；
      // 思考类全局配置在发送时刻取值（thinkingMode ≠ default 才写请求参数）
      const pc = pluginConfig?.value ?? DEFAULT_PLUGIN_CONFIG
      const thinkingOptions: ThinkingOptions = {
        thinkingMode: pc.thinkingMode,
        reasoningEffort: pc.reasoningEffort,
      }
      const request = buildStreamRequest(activeProvider, requestMessages, streamId, thinkingOptions)

      function applyStreamEvent(ev: StreamEvent): void {
        if (ev.chunk) {
          pendingContent += ev.chunk
          scheduleFlush()
        }
        if (ev.reasoning) {
          pendingReasoning += ev.reasoning
          scheduleFlush()
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
            const ev = parseStreamEvent(activeProvider.apiStyle, data)
            if (ev) applyStreamEvent(ev)
          }
        } else if (payload.error) {
          handleStreamError(String(payload.error))
        } else if (payload.done) {
          // 宿主 done 兜底：flush 残留缓冲后终结（已终结时幂等跳过）
          for (const data of sse.flush()) {
            if (streamEnded.value) break
            const ev = parseStreamEvent(activeProvider.apiStyle, data)
            if (ev) applyStreamEvent(ev)
          }
          finishStream(true, undefined, usageAcc, replaceLast)
        }
      })

      context.commands
        .execute('ai-chatbox.chat-stream', { streamId, request })
        .catch((e: any) => handleStreamError(String(e?.message || e)))
    }

    /** 流错误分派：非限流直接收尾；限流在未收到内容且尚有重试额度时进入指数退避 */
    function handleStreamError(errorText: string): void {
      if (!isRateLimitError(errorText) || !nothingReceived()) {
        finishStream(false, errorText, undefined, replaceLast)
        return
      }
      const { maxRetries } = rateLimitParams()
      if (maxRetries <= 0 || rateLimitAttempt >= maxRetries) {
        if (rateLimitAttempt > 0) {
          console.error('[AI Chatbox] Rate limit persists after retries:', errorText)
          finishStream(false, 'mobile.plugin.aiChatbox.rateLimitExhausted', undefined, replaceLast)
        } else {
          // 自动重试关闭（maxRetries=0）：透传原始错误，避免"重试 0 次"的误导文案
          finishStream(false, errorText, undefined, replaceLast)
        }
        return
      }
      scheduleRateLimitRetry(errorText)
    }

    /** 限流退避调度：终结旧尝试的事件消费 → 倒计时（滑出条展示）→ 以新 streamId 重发同一请求 */
    function scheduleRateLimitRetry(errorText: string): void {
      rateLimitAttempt += 1
      const { maxRetries, initialDelayMs, maxDelayMs } = rateLimitParams()
      // 指数退避：1×、2×、4×… initialDelay，封顶 maxDelay
      const delayMs = Math.min(initialDelayMs * 2 ** (rateLimitAttempt - 1), maxDelayMs)

      streamDisposable?.dispose()
      streamDisposable = null
      // 旧尝试的监听已断开（迟到 error/done 不会进入处理）；streamEnded 保持 false，
      // 终止/耗尽走 finishStream 幂等守卫，重试 fire 时直接 startStream 重建监听

      rateLimitRetry.value = {
        attempt: rateLimitAttempt,
        maxRetries,
        countdownSec: Math.ceil(delayMs / 1000),
        status: parseErrorStatus(errorText),
      }
      // 倒计时从绝对截止时间换算（500ms 刷新），不随 interval 调度误差漂移
      const deadline = Date.now() + delayMs
      retryCountdownTimer = setInterval(() => {
        if (!rateLimitRetry.value) return
        rateLimitRetry.value.countdownSec = Math.max(0, Math.ceil((deadline - Date.now()) / 1000))
      }, 500)
      retryFireTimer = setTimeout(() => {
        clearRateLimitTimers()
        rateLimitRetry.value = null
        pendingContent = ''
        pendingReasoning = ''
        startStream()
      }, delayMs)
    }

    startStream()
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
    // 限流重试等待中的任何终态（终止/耗尽/正常兜底）都必须清掉挂起的退避定时器
    clearRateLimitTimers()
    rateLimitRetry.value = null
    // 取消待决 rAF 并立即写回缓冲：落盘/复位的必须是含最后一批 chunk 的终态
    flushStreamingNow()
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
      // 流中断/失败也落盘已接收内容（含思考过程）；重生成时无条件覆盖旧回复行（防旧回复复现）
      if (completed || last.content.trim() || last.reasoning?.trim() || replaceAssistantRow) {
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

  /** 终止限流重试等待（滑出条终止按钮）：取消退避定时器并按"已终止"收尾 */
  function abortRateLimitRetry(): void {
    if (!rateLimitRetry.value) return
    finishStream(false, 'mobile.plugin.aiChatbox.rateLimitAborted')
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
    rateLimitRetry,
    loadConversations,
    loadMessages,
    newConversation,
    renameConversation,
    deleteConversation,
    sendMessage,
    stopGeneration,
    abortRateLimitRetry,
    regenerate,
    switchConversation,
  }
}
