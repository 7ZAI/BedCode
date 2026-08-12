/**
 * AI Chatbox 开发环境 mock（仅 dev-shell 生效，移动端）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册的
 * handler。本模块在 `import.meta.env.DEV`（vite dev server）时经 index.ts 注册
 * 全部命令 handler + 预置供应商配置，使插件在 dev-shell 中展示「已配置 + 有历史」
 * 的完整形态，便于 UI 评审与样式调试；生产构建（vite build）时 DEV=false，
 * 本模块代码不参与打包。
 */
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import { getI18n } from '@bedcode/plugin-sdk-mobile'

// ==================== 宿主 i18n key 补齐（dev-shell 无宿主 locale，运行时由宿主注入） ====================
// 与 bedcode-mobile/src/locales/{zh-CN,en}/mobile.ts 的 plugin.aiChatbox 段同步
// 注意：不能走 context.i18n.registerMessages（会自动加插件 ID 前缀），须直接 merge 宿主全局

const HOST_KEYS_ZH = {
  mobile: {
    plugin: {
      aiChatbox: {
        noProvider: '未配置模型',
        configureModel: '配置模型',
        title: 'AI 对话',
        newConversation: '新对话',
        conversations: '对话',
        noConversations: '暂无对话',
        noConversationsHint: '点击 + 创建第一个对话',
        emptyHint: '支持 DeepSeek / 通义千问 / OpenAI / Anthropic 等供应商',
        rename: '重命名',
        send: '发送',
        stop: '停止',
        model: '模型',
        regenerate: '重新生成',
        inputPlaceholder: '输入消息...（Enter 发送，Shift+Enter 换行）',
        startNewChat: '开始新对话',
        pleaseConfigure: '请先配置 AI 模型',
        you: '我',
        assistant: 'AI',
        thinkingProcess: '思考过程',
        copy: '复制',
        copied: '已复制',
        copyMessage: '复制消息',
        delete: '删除',
        deleteMessage: '删除消息',
        providerConfig: '模型供应商配置',
        backToChat: '返回聊天',
        back: '返回',
        close: '关闭',
        addProvider: '添加供应商',
        editProvider: '编辑供应商',
        selectTemplate: '选择模板',
        customTemplate: '自定义',
        save: '保存',
        saveProvider: '保存',
        deleteProvider: '删除供应商',
        confirmDeleteShort: '确认删除？',
        confirmDeleteTitle: '删除供应商',
        confirmDeleteBody: '确定删除「{name}」？删除后需重新配置',
        noProvidersHint: '暂无供应商，点击上方「添加供应商」开始',
        timeJustNow: '刚刚',
        timeMinutesAgo: '{n} 分钟前',
        timeHoursAgo: '{n} 小时前',
        timeDaysAgo: '{n} 天前',
        activeProvider: '当前使用',
        name: '名称',
        baseUrl: 'Base URL',
        apiKey: 'API Key',
        apiKeyHint: 'API Key 明文存储于本机配置，请妥善保管',
        show: '显示',
        hide: '隐藏',
        modelList: '模型列表',
        addModel: '添加',
        modelId: '输入模型 ID 回车添加',
        noModels: '暂无模型',
        removeModel: '移除模型',
        fetchModels: '拉取模型列表',
        fetchingModels: '拉取中...',
        fetchModelsFailed: '拉取模型失败',
        fetchModelsEmpty: '未获取到模型',
        testConnection: '测试连接',
        testing: '测试中...',
        testOk: '连接正常',
        clear: '清空',
        cancel: '取消',
        contextLimitExceeded: '超出上下文长度，请新建对话',
        authRevoked: '目录授权已失效，请在设置中重新授权',
        requestFailed: '请求失败',
        apiKeyRequired: '请先填写 API Key',
        baseUrlInvalid: 'Base URL 地址无效',
      },
    },
  },
}

const HOST_KEYS_EN = {
  mobile: {
    plugin: {
      aiChatbox: {
        noProvider: 'No model configured',
        configureModel: 'Configure Model',
        title: 'AI Chat',
        newConversation: 'New Chat',
        conversations: 'Conversations',
        noConversations: 'No conversations yet',
        noConversationsHint: 'Click + to create your first chat',
        emptyHint: 'Supports DeepSeek / Qwen / OpenAI / Anthropic providers',
        rename: 'Rename',
        send: 'Send',
        stop: 'Stop',
        model: 'Model',
        regenerate: 'Regenerate',
        inputPlaceholder: 'Type a message... (Enter to send, Shift+Enter for newline)',
        startNewChat: 'Start a new chat',
        pleaseConfigure: 'Configure an AI provider first',
        you: 'You',
        assistant: 'AI',
        thinkingProcess: 'Thinking',
        copy: 'Copy',
        copied: 'Copied',
        copyMessage: 'Copy message',
        delete: 'Delete',
        deleteMessage: 'Delete message',
        providerConfig: 'Provider Settings',
        backToChat: 'Back to chat',
        back: 'Back',
        close: 'Close',
        addProvider: 'Add Provider',
        editProvider: 'Edit Provider',
        selectTemplate: 'Select Template',
        customTemplate: 'Custom',
        save: 'Save',
        saveProvider: 'Save',
        deleteProvider: 'Delete Provider',
        confirmDeleteShort: 'Confirm delete?',
        confirmDeleteTitle: 'Delete Provider',
        confirmDeleteBody: 'Delete "{name}"? You will need to reconfigure it.',
        noProvidersHint: 'No providers yet. Tap "Add Provider" above to get started.',
        timeJustNow: 'Just now',
        timeMinutesAgo: '{n}m ago',
        timeHoursAgo: '{n}h ago',
        timeDaysAgo: '{n}d ago',
        activeProvider: 'Active',
        name: 'Name',
        baseUrl: 'Base URL',
        apiKey: 'API Key',
        apiKeyHint: 'API key is stored in plain text on this device',
        show: 'Show',
        hide: 'Hide',
        modelList: 'Models',
        addModel: 'Add',
        modelId: 'Type a model ID and press Enter',
        noModels: 'No models yet',
        removeModel: 'Remove model',
        fetchModels: 'Fetch Models',
        fetchingModels: 'Fetching...',
        fetchModelsFailed: 'Failed to fetch models',
        fetchModelsEmpty: 'No models returned',
        testConnection: 'Test Connection',
        testing: 'Testing...',
        testOk: 'Connection OK',
        clear: 'Clear',
        cancel: 'Cancel',
        contextLimitExceeded: 'Context length exceeded — start a new conversation',
        authRevoked: 'Directory authorization revoked — re-authorize in settings',
        requestFailed: 'Request failed',
        apiKeyRequired: 'API Key is required',
        baseUrlInvalid: 'Invalid Base URL',
      },
    },
  },
}

// ==================== 模拟状态 ====================

interface MockMessage {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  model?: string
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number }
  reasoning?: string
}

interface MockConversation {
  id: string
  title: string
  createdAt: string
  updatedAt: string
  providerId: string
  providerName: string
  model: string
}

/** 预置供应商（写入 storage，useAiConfig.loadConfig 读取） */
const seedProviders = [
  {
    id: 'prov-deepseek',
    name: 'DeepSeek',
    apiKey: 'sk-test-deepseek',
    baseUrl: 'https://api.deepseek.com/v1',
    apiStyle: 'openai' as const,
    models: ['deepseek-chat', 'deepseek-reasoner'],
    activeModel: 'deepseek-chat',
  },
  {
    id: 'prov-qwen',
    name: '通义千问 (Qwen)',
    apiKey: 'sk-test-qwen',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    apiStyle: 'openai' as const,
    models: ['qwen-turbo', 'qwen-plus', 'qwen-max'],
    activeModel: 'qwen-plus',
  },
]

/** 预置对话（updatedAt 已按 DESC 排列，贴近真实索引文件） */
const seedConversations: MockConversation[] = [
  {
    id: 'conv-rust-async',
    title: 'Rust 异步编程里的超时处理',
    createdAt: '2026-08-10T09:12:00.000Z',
    updatedAt: '2026-08-10T09:20:41.000Z',
    providerId: 'prov-deepseek',
    providerName: 'DeepSeek',
    model: 'deepseek-chat',
  },
  {
    id: 'conv-vue3-composables',
    title: 'Vue3 组合式 API 设计要点',
    createdAt: '2026-08-09T14:30:00.000Z',
    updatedAt: '2026-08-09T15:02:17.000Z',
    providerId: 'prov-qwen',
    providerName: '通义千问 (Qwen)',
    model: 'qwen-plus',
  },
  {
    id: 'conv-weather-widget',
    title: '天气小组件数据源设计讨论',
    createdAt: '2026-08-08T11:00:00.000Z',
    updatedAt: '2026-08-08T11:35:22.000Z',
    providerId: 'prov-deepseek',
    providerName: 'DeepSeek',
    model: 'deepseek-chat',
  },
]

const seedMessages: Record<string, MockMessage[]> = {
  'conv-rust-async': [
    {
      role: 'user',
      content: 'async fn 里怎么优雅处理超时？tokio::time::timeout 的 Err 分支应该怎么处理？',
      timestamp: '2026-08-10T09:12:30.000Z',
    },
    {
      role: 'assistant',
      content:
        '`tokio::time::timeout` 的 `Err(Elapsed)` 分支代表**超时**，处理思路是：把超时当成业务结果而非异常，记录日志后返回兜底值或错误。\n\n```rust\nuse tokio::time::{timeout, Duration};\n\nasync fn call_with_timeout() -> Result<String, AppError> {\n    match timeout(Duration::from_secs(5), slow_request()).await {\n        Ok(Ok(resp)) => Ok(resp),\n        Ok(Err(e)) => Err(e),              // 请求自身失败\n        Err(_) => Err(AppError::Timeout),  // 超时，单独归类\n    }\n}\n```\n\n要点：\n1. 超时与请求失败要**分开处理**，方便调用方区别重试策略\n2. 超时后任务仍在后台运行，可用 `JoinHandle::abort` 主动取消\n3. 高频场景建议把超时时间做成配置项',
      timestamp: '2026-08-10T09:20:41.000Z',
      model: 'deepseek-chat',
      usage: { promptTokens: 486, completionTokens: 312, totalTokens: 798 },
    },
  ],
  'conv-vue3-composables': [
    {
      role: 'user',
      content: '组合式 API 里状态应该放 ref 还是 reactive？团队协作时怎么约定比较好？',
      timestamp: '2026-08-09T14:31:00.000Z',
    },
    {
      role: 'assistant',
      content:
        '建议**默认用 `ref`**，原因是：\n\n- `ref` 可以整体替换（`state.value = newState`），`reactive` 替换会丢失响应性\n- 解构传参时 `ref` 不需要 `toRefs`\n- 泛型与类型推导更直接\n\n```ts\n// 推荐：ref 包裹整个领域状态\nconst filters = ref({ keyword: "", page: 1 })\n\n// 避免：reactive 在解构/替换时踩坑\n// const filters = reactive({ keyword: "", page: 1 })\n```\n\n团队约定：**跨函数共享的状态用 `ref` + 明确的返回对象**，让每个 composable 的返回值结构可预期。',
      timestamp: '2026-08-09T15:02:17.000Z',
      model: 'qwen-plus',
      usage: { promptTokens: 212, completionTokens: 189, totalTokens: 401 },
    },
  ],
  'conv-weather-widget': [
    {
      role: 'user',
      content: '想做一个桌面天气小组件，数据源用聚合 API 好还是单一 API？',
      timestamp: '2026-08-08T11:02:00.000Z',
    },
    {
      role: 'assistant',
      content:
        '取决于使用场景：\n\n| 场景 | 推荐 | 原因 |\n|------|------|------|\n| 快速可用 | 单一 API | 实现简单、请求少 |\n| 多源校验 | 聚合 API | 数据可交叉验证 |\n\n**建议先单一 API**（如 OpenWeatherMap），把解析层抽象成接口，后续换源只改 adapter，不动 UI。',
      timestamp: '2026-08-08T11:35:22.000Z',
      model: 'deepseek-chat',
      usage: { promptTokens: 158, completionTokens: 142, totalTokens: 300 },
    },
  ],
}

/** chat-stream 模拟回复（含 Markdown / 代码块 / 列表，覆盖渲染路径） */
const STREAM_REPLY =
  '这是 **dev-shell mock** 的流式回复，用于验证逐字渲染、Markdown 与代码高亮。\n\n' +
  '```ts\n' +
  'export async function fetchModels(baseUrl: string): Promise<string[]> {\n' +
  '  const res = await fetch(`${baseUrl}/models`, {\n' +
  '    headers: { Authorization: `Bearer ${process.env.API_KEY}` },\n' +
  '  })\n' +
  '  if (!res.ok) throw new Error(`HTTP ${res.status}`)\n' +
  '  const data: { data: Array<{ id: string }> } = await res.json()\n' +
  '  return data.data.map((m) => m.id)\n' +
  '}\n' +
  '```\n\n' +
  '要点：\n' +
  '1. 每个 chunk 追加到 streamingContent\n' +
  '2. done 事件携带 usage（↑prompt ↓completion Σtotal）\n' +
  '3. 停止 / 重生成走本地截断与文件覆盖'

// ==================== 运行态状态（默认空；?mock=1 时 seedMockData 填充） ====================

const conversations: MockConversation[] = []
const messagesByConv: Record<string, MockMessage[]> = {}

// ==================== 命令 handler ====================

const timers: number[] = []

function registerCommands(context: PluginContext): void {
  context.commands.register('ai-chatbox.list-conversations', () => ({
    conversations: conversations.map((c) => ({ ...c })),
  }))

  context.commands.register('ai-chatbox.get-messages', (args: any) => {
    const list = messagesByConv[args?.conversationId] || []
    return { messages: list.map((m) => ({ ...m })) }
  })

  context.commands.register('ai-chatbox.save-conversation', (args: any) => {
    const conv = args?.conversation as MockConversation | undefined
    if (!conv) return { ok: false }
    const idx = conversations.findIndex((c) => c.id === conv.id)
    if (idx === -1) {
      conversations.unshift({ ...conv })
    } else {
      conversations[idx] = { ...conv }
    }
    return { ok: true }
  })

  context.commands.register('ai-chatbox.save-message', (args: any) => {
    const { conversationId, role, content, timestamp, model, usage, reasoning } = args || {}
    if (!conversationId) return { ok: false }
    if (!messagesByConv[conversationId]) messagesByConv[conversationId] = []
    messagesByConv[conversationId].push({
      role,
      content,
      timestamp,
      model: model || undefined,
      usage: usage || undefined,
      reasoning: reasoning || undefined,
    })
    return { ok: true }
  })

  context.commands.register('ai-chatbox.delete-conversation', (args: any) => {
    const id = args?.conversationId
    const idx = conversations.findIndex((c) => c.id === id)
    if (idx !== -1) conversations.splice(idx, 1)
    delete messagesByConv[id]
    return { ok: true }
  })

  context.commands.register('ai-chatbox.chat-stream', (args: any) => {
    const streamId = args?.streamId as string | undefined
    if (!streamId) return { ok: false }
    // 与真实宿主 raw 模式一致：逐网络 chunk 推原始 SSE 字节（openai 方言
    // data 行），前端 SseBuffer + adapter 自行解析；结尾补 usage 尾块与 [DONE]
    let i = 0
    const tick = () => {
      const step = 6 + Math.floor(Math.random() * 7)
      const textChunk = STREAM_REPLY.slice(i, i + step)
      i += step
      if (i >= STREAM_REPLY.length) {
        clearInterval(handle)
        const hIdx = timers.indexOf(handle)
        if (hIdx !== -1) timers.splice(hIdx, 1)
        const payload = {
          chunk:
            `data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: 421, completion_tokens: 356, total_tokens: 777 } })}\n\n` +
            'data: [DONE]\n\n',
        }
        context.events.emit(`ai-chatbox:stream:${streamId}`, payload)
        context.events.emit(`ai-chatbox:stream:${streamId}`, { done: true })
      } else {
        context.events.emit(`ai-chatbox:stream:${streamId}`, {
          chunk: `data: ${JSON.stringify({ choices: [{ delta: { content: textChunk } }] })}\n\n`,
        })
      }
    }
    const handle = setInterval(tick, 30) as unknown as number
    timers.push(handle)
    return { ok: true }
  })

  context.commands.register('ai-chatbox.chat-complete', () => ({
    status: 200,
    body: JSON.stringify({ choices: [{ message: { content: 'mock 测试连接回复：网络链路正常 ✅' } }] }),
  }))

  context.commands.register('ai-chatbox.fetch-models', () => ({
    status: 200,
    body: JSON.stringify({ data: [{ id: 'deepseek-chat' }, { id: 'deepseek-reasoner' }, { id: 'deepseek-v3' }] }),
  }))
}

// ==================== 注入入口 ====================

let mockRegistered = false

/** URL 参数 ?mock=1 开启预置数据（空态默认，便于分别评审） */
function isMockDataEnabled(): boolean {
  if (typeof window === 'undefined') return false
  return new URLSearchParams(window.location.search).get('mock') === '1'
}

/** 填充预置数据（对话/消息/供应商 storage） */
async function seedMockData(context: PluginContext): Promise<void> {
  conversations.push(...seedConversations.map((c) => ({ ...c })))
  for (const [id, list] of Object.entries(seedMessages)) {
    messagesByConv[id] = list.map((m) => ({ ...m }))
  }
  await context.storage.set('apiProviders', seedProviders)
  await context.storage.set('activeProvider', 'prov-deepseek')
  await context.storage.set('activeModel', 'deepseek-chat')
}

/** 注册 dev mock（幂等）：补齐宿主 i18n + 命令 handler（+ ?mock=1 预置数据） */
export async function registerDevMock(context: PluginContext): Promise<void> {
  if (mockRegistered) return
  mockRegistered = true

  // 补齐宿主 mobile.plugin.aiChatbox 文案（真实运行时由宿主注入）
  const hostI18n = getI18n()
  if (hostI18n?.global?.mergeLocaleMessage) {
    hostI18n.global.mergeLocaleMessage('zh-CN', HOST_KEYS_ZH)
    hostI18n.global.mergeLocaleMessage('en', HOST_KEYS_EN)
  }

  registerCommands(context)

  if (isMockDataEnabled()) {
    await seedMockData(context)
    console.log('[AI Chatbox] dev-shell mock 已注册（命令 + 预置数据）')
  } else {
    console.log('[AI Chatbox] dev-shell mock 已注册（仅命令，空态展示）')
  }
}

/** 停止模拟定时器 */
export function disposeDevMock(): void {
  while (timers.length) clearInterval(timers.pop()!)
}
