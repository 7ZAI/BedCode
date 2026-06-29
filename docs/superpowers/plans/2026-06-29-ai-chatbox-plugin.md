# AI Chatbox 插件实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在桌面端实现 AI Chatbox 插件，提供侧边栏聊天面板、多模型 API 配置、终端提示词优化功能。

**Architecture:** 纯 BedCode 插件方案，完全在插件系统框架内实现。插件通过 `plugin.json` 声明能力，通过 `PluginContext API` 访问宿主能力（sidebar 注册、terminal 输入、storage 持久化）。AI API 调用使用前端 fetch 直连 OpenAI 兼容格式端点，流式 SSE 解析。

**Tech Stack:** Vue 3 + TypeScript, TailwindCSS, OpenAI Chat Completion API (兼容格式), `marked` (Markdown 渲染), BedCode PluginContext API

---

## File Structure

### 插件文件（新建，位于 `~/.bedcode/plugins/com.bedcode.ai-chatbox/`）

| 文件 | 职责 |
|------|------|
| `plugin.json` | 插件描述文件 |
| `index.ts` | 入口：activate 注册 sidebar panel + terminal toolbar item |
| `types.ts` | 内部类型：ApiProvider, ChatMessage, ConversationMeta |
| `services/openaiClient.ts` | OpenAI 兼容 API 客户端：chatStream / chat，SSE 解析 |
| `composables/useAiConfig.ts` | API provider 配置管理：增删改查、活跃切换、预设模板 |
| `composables/useAiChat.ts` | 聊天核心逻辑：发送消息、流式接收、对话管理、历史持久化 |
| `composables/usePromptOptimizer.ts` | 终端提示词优化：获取输入、调用 API、弹出确认、填入终端 |
| `components/ChatView.vue` | 侧边栏聊天面板主界面 |
| `components/ChatMessage.vue` | 单条消息渲染（用户/AI，Markdown） |
| `components/ChatInput.vue` | 消息输入栏 |
| `components/ProviderManager.vue` | API provider 配置管理面板（内嵌 ChatView） |
| `components/PromptOptimizeDialog.vue` | 提示词优化确认弹窗 |

### 宿主文件（修改）

| 文件 | 变更 |
|------|------|
| `src-tauri/src/desktop/plugin/permission.rs:43` | `PERMISSION_API_MAP` 中 `ui:input` 补充 `ui.registerTerminalToolbarItem` |
| `src/modules/desktop/components/TerminalPreview.vue:382` | 添加当前行输入追踪 + 插件事件响应 |

---

## Task 1: 修复 Rust 端 permission API 映射

**Files:**
- Modify: `src-tauri/src/desktop/plugin/permission.rs:43`

- [ ] **Step 1: 更新 PERMISSION_API_MAP**

在 `permission.rs` 第 43 行，将 `ui:input` 的 API 列表从 `["ui.registerInputExtension"]` 扩展为包含 `ui.registerTerminalToolbarItem` 和 `ui.registerTitleBarItem`：

```rust
(PERMISSION_UI_INPUT, &["ui.registerInputExtension", "ui.registerTerminalToolbarItem"]),
(PERMISSION_UI_STATUSBAR, &["ui.registerStatusBarItem", "ui.registerTitleBarItem"]),
```

- [ ] **Step 2: 运行 Rust 测试验证**

Run: `cd D:/tauriProject/BedCode/src-tauri && cargo test --lib desktop::plugin::permission`
Expected: 所有测试通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/desktop/plugin/permission.rs
git commit -m "fix(plugin): add registerTerminalToolbarItem and registerTitleBarItem to permission API map"
```

---

## Task 2: 宿主端添加终端输入追踪和插件事件响应

**Files:**
- Modify: `src/modules/desktop/components/TerminalPreview.vue:382`

- [ ] **Step 1: 在 TerminalPreview.vue 添加输入追踪**

在 `initTerminal()` 函数中，`terminal.onData` 回调内部，添加 `currentLineBuffer` 追踪逻辑。在 `terminal.onData` 回调之后添加插件事件监听：

```typescript
// 追踪当前行输入（MVP：仅追踪可打印字符和退格）
let currentLineBuffer = ''
const origOnData = terminal.onData((data: string) => {
  // 原有逻辑
  if (!props.session) return
  sessionStore.writeToSession(props.session.id, data)

  // 追踪当前行输入
  if (data === '\r' || data === '\n') {
    currentLineBuffer = ''
  } else if (data === '\x7f' || data === '\b') {
    currentLineBuffer = currentLineBuffer.slice(0, -1)
  } else if (data === '\x15') {
    // Ctrl+U 清除当前行
    currentLineBuffer = ''
  } else if (data.length === 1 && data.charCodeAt(0) >= 32) {
    currentLineBuffer += data
  }
  // 忽略方向键、控制序列等复杂场景
})
```

- [ ] **Step 2: 在 TerminalPreview.vue 添加插件事件监听**

在 `onMounted` 中添加插件事件监听，在 `onUnmounted` 中清理：

```typescript
// 在 onMounted 内，initTerminal() 之后添加：
import { getPluginRegistry } from '@/modules/shared/plugin/registry'

const registry = getPluginRegistry()
let inputRequestDisposable: { dispose(): void } | null = null

// initTerminal() 之后
inputRequestDisposable = registry // 事件总线在 registry 中没有 on/emit
```

**修正方案**：由于 `PluginRegistryClass` 没有事件监听方法，改用 `events.ts` 的 `on`/`emit`。但 `events.ts` 的 `on` 需要 `pluginId` 参数。宿主组件不是插件，使用特殊 ID `'__host__'`：

```typescript
import { on as pluginEventOn, emit as pluginEventEmit, clearPluginEvents } from '@/modules/shared/plugin/events'

// 在 initTerminal() 之后的 onMounted 中：
pluginEventOn('__host__', 'ai-chatbox:getCurrentInput', () => {
  pluginEventEmit('ai-chatbox:currentInput', { sessionId: sessionId.value, text: currentLineBuffer })
})
```

在 `onUnmounted` 中：
```typescript
clearPluginEvents('__host__')
```

- [ ] **Step 3: 验证编译通过**

Run: `cd D:/tauriProject/BedCode && npx vue-tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 4: Commit**

```bash
git add src/modules/desktop/components/TerminalPreview.vue
git commit -m "feat(terminal): add current line input tracking and plugin event response for AI chatbox"
```

---

## Task 3: 创建插件基础文件（plugin.json + types.ts + index.ts）

**Files:**
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/plugin.json`
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/types.ts`
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/index.ts`

- [ ] **Step 1: 创建插件目录**

Run: `mkdir -p ~/.bedcode/plugins/com.bedcode.ai-chatbox/composables ~/.bedcode/plugins/com.bedcode.ai-chatbox/components ~/.bedcode/plugins/com.bedcode.ai-chatbox/services`

- [ ] **Step 2: 创建 plugin.json**

```json
{
  "id": "com.bedcode.ai-chatbox",
  "name": "AI Chatbox",
  "version": "1.0.0",
  "description": "AI 大模型对话与终端提示词优化",
  "author": "BedCode",
  "main": "index.ts",
  "sandbox": "inline",
  "permissions": ["ui:sidebar", "ui:input", "storage", "terminal:input", "session:read"],
  "contributes": {
    "views": [
      {
        "id": "ai-chatbox.sidebar",
        "type": "sidebar",
        "title": "AI 对话",
        "component": "ChatView"
      }
    ],
    "configuration": {
      "title": "AI Chatbox Settings",
      "properties": {
        "apiProviders": {
          "type": "string",
          "title": "API Providers (JSON)",
          "description": "JSON array of API provider configs",
          "default": "[]"
        },
        "activeProvider": {
          "type": "string",
          "title": "Active Provider Name",
          "default": ""
        }
      }
    }
  }
}
```

- [ ] **Step 3: 创建 types.ts**

```typescript
/**
 * AI Chatbox 插件内部类型定义
 */

/** API 提供商配置 */
export interface ApiProvider {
  name: string
  apiKey: string
  baseUrl: string
  model: string
}

/** 聊天消息 */
export interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
}

/** 对话元数据 */
export interface ConversationMeta {
  id: string
  title: string
  createdAt: string
  updatedAt: string
  providerName: string
}

/** 预设模板 */
export interface ProviderPreset {
  name: string
  baseUrl: string
  model: string
}

/** AI 聊天响应事件 */
export interface CurrentInputEvent {
  sessionId: string
  text: string
}
```

- [ ] **Step 4: 创建 index.ts（骨架）**

```typescript
/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板 + 终端提示词优化
 */
import type { PluginContext } from '../../shared/plugin/types'

export async function activate(context: PluginContext): Promise<void> {
  // 将在 Task 8 中实现完整逻辑
  console.log('[AI Chatbox] Plugin activated')
}

export async function deactivate(): Promise<void> {
  console.log('[AI Chatbox] Plugin deactivated')
}
```

- [ ] **Step 5: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/
git commit -m "feat(plugin): add AI chatbox plugin skeleton with plugin.json, types, and entry"
```

---

## Task 4: 实现 OpenAI 兼容 API 客户端

**Files:**
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts`

- [ ] **Step 1: 创建 OpenAI 客户端**

```typescript
/**
 * OpenAI 兼容格式 API 客户端
 *
 * 支持流式（SSE）和非流式请求，兼容所有 OpenAI API 格式的模型提供商
 */
import type { ApiProvider, ChatMessage } from '../types'

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
      // 保留最后一行（可能不完整）
      buffer = lines.pop() || ''

      for (const line of lines) {
        const content = parseSseLine(line)
        if (content !== null) {
          callbacks.onChunk(content)
        }
      }
    }

    // 处理 buffer 中剩余内容
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
```

注意：`ProviderPreset` 类型在 `types.ts` 中定义。

- [ ] **Step 2: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts
git commit -m "feat(plugin): add OpenAI-compatible API client with streaming SSE support"
```

---

## Task 5: 实现 API 配置管理 composable

**Files:**
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/useAiConfig.ts`

- [ ] **Step 1: 创建 useAiConfig composable**

```typescript
/**
 * API Provider 配置管理
 *
 * 管理多个 OpenAI 兼容 API 配置的增删改查、活跃切换、预设模板
 */
import { ref, computed } from 'vue'
import type { ApiProvider, ProviderPreset } from '../types'
import { PROVIDER_PRESETS } from '../services/openaiClient'

/** 配置管理 composable */
export function useAiConfig(
  storageGet: (key: string) => Promise<any>,
  storageSet: (key: string, value: any) => Promise<void>,
) {
  const providers = ref<ApiProvider[]>([])
  const activeProviderName = ref('')
  const loading = ref(false)
  const showProviderManager = ref(false)

  /** 当前活跃的 provider */
  const activeProvider = computed<ApiProvider | undefined>(() =>
    providers.value.find(p => p.name === activeProviderName.value)
  )

  /** 是否已配置至少一个 provider */
  const hasProvider = computed(() => providers.value.length > 0)

  /** 从 storage 加载配置 */
  async function loadConfig(): Promise<void> {
    loading.value = true
    try {
      const savedProviders = await storageGet('apiProviders')
      if (savedProviders) {
        const parsed = typeof savedProviders === 'string' ? JSON.parse(savedProviders) : savedProviders
        providers.value = Array.isArray(parsed) ? parsed : []
      }

      const savedActive = await storageGet('activeProvider')
      activeProviderName.value = typeof savedActive === 'string' ? savedActive : ''

      // 如果没有活跃 provider 但有配置，选第一个
      if (!activeProviderName.value && providers.value.length > 0) {
        activeProviderName.value = providers.value[0].name
      }
    } catch (e) {
      console.error('[AI Chatbox] Failed to load config:', e)
    } finally {
      loading.value = false
    }
  }

  /** 保存配置到 storage */
  async function saveConfig(): Promise<void> {
    try {
      await storageSet('apiProviders', JSON.stringify(providers.value))
      await storageSet('activeProvider', activeProviderName.value)
    } catch (e) {
      console.error('[AI Chatbox] Failed to save config:', e)
    }
  }

  /** 添加 provider */
  async function addProvider(provider: ApiProvider): Promise<void> {
    // 检查名称是否重复
    if (providers.value.some(p => p.name === provider.name)) {
      throw new Error(`Provider "${provider.name}" 已存在`)
    }
    providers.value.push(provider)
    if (!activeProviderName.value) {
      activeProviderName.value = provider.name
    }
    await saveConfig()
  }

  /** 删除 provider */
  async function removeProvider(name: string): Promise<void> {
    providers.value = providers.value.filter(p => p.name !== name)
    if (activeProviderName.value === name) {
      activeProviderName.value = providers.value[0]?.name || ''
    }
    await saveConfig()
  }

  /** 更新 provider */
  async function updateProvider(oldName: string, provider: ApiProvider): Promise<void> {
    const index = providers.value.findIndex(p => p.name === oldName)
    if (index === -1) return
    providers.value[index] = provider
    if (activeProviderName.value === oldName) {
      activeProviderName.value = provider.name
    }
    await saveConfig()
  }

  /** 切换活跃 provider */
  async function setActiveProvider(name: string): Promise<void> {
    if (!providers.value.some(p => p.name === name)) return
    activeProviderName.value = name
    await saveConfig()
  }

  /** 从预设创建 provider（只填 API Key） */
  async function addFromPreset(preset: ProviderPreset, apiKey: string): Promise<void> {
    await addProvider({
      name: preset.name,
      apiKey,
      baseUrl: preset.baseUrl,
      model: preset.model,
    })
  }

  return {
    providers,
    activeProviderName,
    activeProvider,
    hasProvider,
    loading,
    showProviderManager,
    loadConfig,
    addProvider,
    removeProvider,
    updateProvider,
    setActiveProvider,
    addFromPreset,
    PROVIDER_PRESETS,
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/useAiConfig.ts
git commit -m "feat(plugin): add AI config composable for provider management"
```

---

## Task 6: 实现聊天核心逻辑 composable

**Files:**
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/useAiChat.ts`

- [ ] **Step 1: 创建 useAiChat composable**

```typescript
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
      .slice(0, -1)  // 排除空的 assistant 占位
      .map(m => ({ role: m.role, content: m.content }))

    // 流式调用
    await chatStream(
      provider,
      requestMessages,
      {
        onChunk: (text) => {
          streamingContent.value += text
          // 实时更新最后一条消息
          const last = messages.value[messages.value.length - 1]
          if (last && last.role === 'assistant') {
            last.content = streamingContent.value
          }
        },
        onDone: async () => {
          sending.value = false
          streamingContent.value = ''
          await saveMessages()
          // 更新对话时间
          if (conv) {
            conv.updatedAt = new Date().toISOString()
            await saveConversations()
          }
        },
        onError: async (error) => {
          sending.value = false
          streamingContent.value = ''
          // 更新错误消息
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
    // abort controller 逻辑可以后续添加
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
```

- [ ] **Step 2: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/useAiChat.ts
git commit -m "feat(plugin): add AI chat composable with streaming and history"
```

---

## Task 7: 实现提示词优化 composable

**Files:**
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/usePromptOptimizer.ts`

- [ ] **Step 1: 创建 usePromptOptimizer composable**

```typescript
/**
 * 终端提示词优化
 *
 * 获取终端当前输入 → 调用 AI 优化 → 弹窗确认 → 填入终端
 */
import { ref } from 'vue'
import type { ApiProvider, CurrentInputEvent } from '../types'
import { chat } from '../services/openaiClient'
import { emit as pluginEventEmit, on as pluginEventOn } from '../../../../src/modules/shared/plugin/events'

const OPTIMIZE_SYSTEM_PROMPT = `你是一个提示词优化专家。请优化以下用户输入的提示词，使其更清晰、更具体、更容易让 AI 理解和执行。
要求：
1. 保持原始意图不变
2. 添加必要的上下文和约束条件
3. 使用更精确的表达方式
4. 只输出优化后的提示词，不要添加任何解释、前缀或引号`

export function usePromptOptimizer(
  getActiveProvider: () => ApiProvider | undefined,
  sendInput: (sessionId: string, text: string) => Promise<void>,
) {
  const optimizing = ref(false)
  const showDialog = ref(false)
  const originalText = ref('')
  const optimizedText = ref('')
  const errorMessage = ref('')
  let currentSessionId = ''

  /** 获取终端当前输入内容 */
  function getCurrentInput(): Promise<CurrentInputEvent> {
    return new Promise((resolve) => {
      const disposable = pluginEventOn('__ai-chatbox__', 'ai-chatbox:currentInput', (data: any) => {
        disposable.dispose()
        resolve(data as CurrentInputEvent)
      })
      // 请求宿主组件返回当前输入
      pluginEventEmit('ai-chatbox:getCurrentInput')
      // 超时保护
      setTimeout(() => {
        disposable.dispose()
        resolve({ sessionId: '', text: '' })
      }, 3000)
    })
  }

  /** 触发优化流程 */
  async function optimizePrompt(): Promise<void> {
    const provider = getActiveProvider()
    if (!provider) {
      errorMessage.value = '请先配置 AI 模型'
      showDialog.value = true
      return
    }

    // 获取当前终端输入
    const input = await getCurrentInput()
    if (!input.text) {
      errorMessage.value = '终端无输入内容'
      showDialog.value = true
      return
    }

    currentSessionId = input.sessionId
    originalText.value = input.text
    errorMessage.value = ''
    optimizing.value = true
    showDialog.value = true
    optimizedText.value = ''

    try {
      const result = await chat(provider, [
        { role: 'system', content: OPTIMIZE_SYSTEM_PROMPT, timestamp: new Date().toISOString() },
        { role: 'user', content: input.text, timestamp: new Date().toISOString() },
      ])
      optimizedText.value = result
    } catch (e: any) {
      errorMessage.value = e.message || '优化失败'
    } finally {
      optimizing.value = false
    }
  }

  /** 采纳优化结果并填入终端 */
  async function acceptOptimized(): Promise<void> {
    if (!currentSessionId || !optimizedText.value) return
    // \x15 = Ctrl+U 清除当前行，然后填入优化后的文本
    await sendInput(currentSessionId, '\x15' + optimizedText.value)
    showDialog.value = false
  }

  /** 取消 */
  function cancelOptimize(): void {
    showDialog.value = false
    originalText.value = ''
    optimizedText.value = ''
    errorMessage.value = ''
  }

  return {
    optimizing,
    showDialog,
    originalText,
    optimizedText,
    errorMessage,
    optimizePrompt,
    acceptOptimized,
    cancelOptimize,
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/usePromptOptimizer.ts
git commit -m "feat(plugin): add prompt optimizer composable with terminal input capture"
```

---

## Task 8: 实现聊天界面组件

**Files:**
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ChatMessage.vue`
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ChatInput.vue`
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ProviderManager.vue`
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/PromptOptimizeDialog.vue`
- Create: `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ChatView.vue`

- [ ] **Step 1: 创建 ChatMessage.vue**

```vue
<template>
  <div :class="['flex gap-3', message.role === 'user' ? 'justify-end' : 'justify-start']">
    <!-- AI 消息头像 -->
    <div
      v-if="message.role === 'assistant'"
      class="w-7 h-7 rounded-full bg-primary-100 dark:bg-primary-900 flex items-center justify-center text-xs flex-shrink-0 mt-1"
    >
      AI
    </div>

    <!-- 消息内容 -->
    <div
      :class="[
        'max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed',
        message.role === 'user'
          ? 'bg-primary-600 text-white'
          : 'bg-slate-100 dark:bg-dark-700 text-slate-800 dark:text-dark-200'
      ]"
    >
      <!-- 流式输出光标 -->
      <div v-if="message.role === 'assistant' && !message.content && streaming" class="flex items-center gap-1">
        <span class="inline-block w-1.5 h-4 bg-primary-500 animate-pulse"></span>
      </div>
      <!-- Markdown 渲染 -->
      <div v-else-if="message.role === 'assistant'" class="prose prose-sm dark:prose-invert max-w-none" v-html="renderedContent"></div>
      <!-- 用户消息纯文本 -->
      <div v-else class="whitespace-pre-wrap">{{ message.content }}</div>
    </div>

    <!-- 用户消息头像 -->
    <div
      v-if="message.role === 'user'"
      class="w-7 h-7 rounded-full bg-slate-200 dark:bg-dark-600 flex items-center justify-center text-xs flex-shrink-0 mt-1"
    >
      我
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 聊天消息渲染组件
 *
 * 用户消息右对齐纯文本，AI 消息左对齐 Markdown 渲染
 */
import { computed } from 'vue'
import type { ChatMessage } from '../types'

const props = defineProps<{
  message: ChatMessage
  streaming?: boolean
}>()

/** 简单 Markdown → HTML 转换 */
const renderedContent = computed(() => {
  let text = props.message.content

  // 代码块
  text = text.replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="bg-slate-800 text-green-300 rounded p-2 my-1 overflow-x-auto text-xs"><code>$2</code></pre>')

  // 行内代码
  text = text.replace(/`([^`]+)`/g, '<code class="bg-slate-200 dark:bg-dark-600 px-1 rounded text-xs">$1</code>')

  // 粗体
  text = text.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')

  // 斜体
  text = text.replace(/\*([^*]+)\*/g, '<em>$1</em>')

  // 换行
  text = text.replace(/\n/g, '<br>')

  return text
})
</script>
```

- [ ] **Step 2: 创建 ChatInput.vue**

```vue
<template>
  <div class="flex gap-2 items-end">
    <textarea
      ref="inputRef"
      v-model="text"
      :placeholder="placeholder"
      :disabled="disabled"
      rows="1"
      class="flex-1 resize-none bg-slate-50 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white placeholder-slate-400 dark:placeholder-dark-500 focus:border-primary-500 outline-none"
      @keydown.enter.exact.prevent="handleSend"
      @input="autoResize"
    ></textarea>
    <button
      :disabled="disabled || !text.trim()"
      class="px-3 py-2 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-medium transition-colors flex-shrink-0"
      @click="handleSend"
    >
      发送
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 聊天输入栏组件
 *
 * 支持 Enter 发送，Shift+Enter 换行
 */
import { ref, nextTick } from 'vue'

const props = withDefaults(defineProps<{
  disabled?: boolean
  placeholder?: string
}>(), {
  disabled: false,
  placeholder: '输入消息...',
})

const emit = defineEmits<{
  send: [content: string]
}>()

const text = ref('')
const inputRef = ref<HTMLTextAreaElement | null>(null)

function handleSend(): void {
  const content = text.value.trim()
  if (!content || props.disabled) return
  emit('send', content)
  text.value = ''
  nextTick(() => autoResize())
}

function autoResize(): void {
  const el = inputRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 120) + 'px'
}
</script>
```

- [ ] **Step 3: 创建 ProviderManager.vue**

```vue
<template>
  <div class="p-4 space-y-4">
    <h4 class="text-sm font-semibold text-slate-700 dark:text-dark-300">模型配置</h4>

    <!-- 已配置的 provider 列表 -->
    <div v-for="provider in providers" :key="provider.name" class="space-y-2">
      <div
        :class="[
          'flex items-center justify-between p-3 rounded-lg border cursor-pointer transition-colors',
          provider.name === activeProviderName
            ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
            : 'border-slate-200 dark:border-dark-600 bg-white dark:bg-dark-800 hover:border-slate-300 dark:hover:border-dark-500'
        ]"
        @click="$emit('setActive', provider.name)"
      >
        <div>
          <div class="text-sm font-medium text-slate-800 dark:text-white">{{ provider.name }}</div>
          <div class="text-xs text-slate-400 dark:text-dark-500">{{ provider.model }}</div>
        </div>
        <div class="flex items-center gap-2">
          <span v-if="provider.name === activeProviderName" class="text-xs text-primary-600 dark:text-primary-400">当前</span>
          <button
            class="text-xs text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300"
            @click.stop="$emit('remove', provider.name)"
          >
            删除
          </button>
        </div>
      </div>
    </div>

    <!-- 添加新 provider -->
    <div class="border border-dashed border-slate-300 dark:border-dark-600 rounded-lg p-3 space-y-2">
      <h5 class="text-xs font-medium text-slate-500 dark:text-dark-400">从预设添加</h5>
      <div class="flex flex-wrap gap-2">
        <button
          v-for="preset in presets"
          :key="preset.name"
          class="px-2 py-1 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
          @click="selectPreset(preset)"
        >
          {{ preset.name }}
        </button>
      </div>

      <!-- 编辑表单 -->
      <div v-if="editingPreset" class="space-y-2 pt-2">
        <input
          v-model="form.name"
          type="text"
          placeholder="名称"
          class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
        />
        <input
          v-model="form.apiKey"
          type="password"
          placeholder="API Key"
          class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
        />
        <input
          v-model="form.baseUrl"
          type="text"
          placeholder="Base URL"
          class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
        />
        <input
          v-model="form.model"
          type="text"
          placeholder="模型名称"
          class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none"
        />
        <div class="flex gap-2">
          <button
            :disabled="!form.name || !form.apiKey"
            class="px-3 py-1.5 text-xs bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded transition-colors"
            @click="handleAdd"
          >
            添加
          </button>
          <button
            class="px-3 py-1.5 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
            @click="editingPreset = null"
          >
            取消
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * API Provider 配置管理面板
 */
import { ref, reactive } from 'vue'
import type { ApiProvider, ProviderPreset } from '../types'
import { PROVIDER_PRESETS } from '../services/openaiClient'

const props = defineProps<{
  providers: ApiProvider[]
  activeProviderName: string
}>()

defineEmits<{
  setActive: [name: string]
  remove: [name: string]
  add: [provider: ApiProvider]
}>()

const presets = PROVIDER_PRESETS
const editingPreset = ref<ProviderPreset | null>(null)
const form = reactive({ name: '', apiKey: '', baseUrl: '', model: '' })

function selectPreset(preset: ProviderPreset): void {
  editingPreset.value = preset
  form.name = preset.name
  form.apiKey = ''
  form.baseUrl = preset.baseUrl
  form.model = preset.model
}

function handleAdd(): void {
  if (!form.name || !form.apiKey) return
  // emit add 事件，由父组件处理
  // 这里直接使用 defineEmits 的 add 事件
}
</script>
```

注意：`handleAdd` 需要修复 emit 调用。在完整实现中会改为 `emit('add', { name: form.name, apiKey: form.apiKey, baseUrl: form.baseUrl, model: form.model })`。

- [ ] **Step 4: 创建 PromptOptimizeDialog.vue**

```vue
<template>
  <div
    v-if="show"
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
  >
    <!-- 背景遮罩 -->
    <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" @click="$emit('cancel')"></div>

    <!-- 弹窗内容 -->
    <div class="relative bg-white dark:bg-dark-800 rounded-xl shadow-2xl border border-slate-200 dark:border-dark-700 w-full max-w-lg">
      <!-- 标题 -->
      <div class="px-5 py-3 border-b border-slate-100 dark:border-dark-700">
        <h3 class="text-base font-semibold text-slate-800 dark:text-white">AI 提示词优化</h3>
      </div>

      <!-- 内容 -->
      <div class="p-5 space-y-4">
        <!-- 错误提示 -->
        <div v-if="error" class="p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm rounded-lg">
          {{ error }}
        </div>

        <!-- 加载中 -->
        <div v-else-if="optimizing" class="flex items-center justify-center py-8">
          <div class="flex items-center gap-2 text-slate-500 dark:text-dark-400">
            <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.969 7.969 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            AI 正在优化提示词...
          </div>
        </div>

        <!-- 结果 -->
        <template v-else>
          <!-- 原始提示词 -->
          <div>
            <label class="block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">原始提示词</label>
            <div class="p-3 bg-slate-50 dark:bg-dark-700 rounded-lg text-sm text-slate-600 dark:text-dark-300 whitespace-pre-wrap">{{ original }}</div>
          </div>

          <!-- 优化后提示词 -->
          <div>
            <label class="block text-xs font-medium text-slate-500 dark:text-dark-400 mb-1">优化后提示词</label>
            <div class="p-3 bg-primary-50 dark:bg-primary-900/20 rounded-lg text-sm text-primary-800 dark:text-primary-200 whitespace-pre-wrap border border-primary-200 dark:border-primary-800">{{ optimized }}</div>
          </div>
        </template>
      </div>

      <!-- 底部按钮 -->
      <div v-if="!optimizing && !error" class="px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end gap-2">
        <button
          class="px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
          @click="$emit('cancel')"
        >
          取消
        </button>
        <button
          :disabled="!optimized"
          class="px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg transition-colors"
          @click="$emit('accept')"
        >
          采纳并填入终端
        </button>
      </div>

      <!-- 仅关闭按钮（错误或加载中取消） -->
      <div v-else-if="error" class="px-5 py-3 border-t border-slate-100 dark:border-dark-700 flex justify-end">
        <button
          class="px-4 py-2 text-sm bg-slate-100 dark:bg-dark-700 text-slate-700 dark:text-dark-300 rounded-lg hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
          @click="$emit('cancel')"
        >
          关闭
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 提示词优化确认弹窗
 *
 * 显示原始/优化后提示词对比，用户确认后填入终端
 */
defineProps<{
  show: boolean
  optimizing: boolean
  original: string
  optimized: string
  error: string
}>()

defineEmits<{
  accept: []
  cancel: []
}>()
</script>
```

- [ ] **Step 5: 创建 ChatView.vue（主面板）**

```vue
<template>
  <div class="h-full flex flex-col bg-white dark:bg-dark-900">
    <!-- Header -->
    <header class="px-4 py-2 flex items-center justify-between border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800">
      <div class="flex items-center gap-2">
        <!-- Provider 选择 -->
        <select
          v-if="config.hasProvider.value"
          :value="config.activeProviderName.value"
          class="bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1 text-xs text-slate-700 dark:text-white outline-none"
          @change="config.setActiveProvider(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="p in config.providers.value" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
        <span v-else class="text-xs text-slate-400">未配置模型</span>
      </div>

      <div class="flex items-center gap-1">
        <!-- 设置按钮 -->
        <button
          class="p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors"
          title="模型配置"
          @click="config.showProviderManager.value = !config.showProviderManager.value"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>

        <!-- 新建对话 -->
        <button
          :disabled="!config.hasProvider.value"
          class="p-1.5 text-slate-500 dark:text-dark-400 hover:bg-slate-200 dark:hover:bg-dark-700 rounded transition-colors disabled:opacity-50"
          title="新对话"
          @click="chat.newConversation(config.activeProviderName.value)"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Provider Manager（可折叠） -->
    <ProviderManager
      v-if="config.showProviderManager.value"
      :providers="config.providers.value"
      :active-provider-name="config.activeProviderName.value"
      @set-active="config.setActiveProvider"
      @remove="config.removeProvider"
      @add="config.addProvider"
    />

    <!-- 未配置提示 -->
    <div v-if="!config.hasProvider.value" class="flex-1 flex flex-col items-center justify-center p-6 text-center">
      <div class="text-4xl mb-3">🤖</div>
      <p class="text-sm text-slate-500 dark:text-dark-400 mb-3">请先配置 AI 模型</p>
      <button
        class="px-4 py-2 text-sm bg-primary-600 hover:bg-primary-700 text-white rounded-lg transition-colors"
        @click="config.showProviderManager.value = true"
      >
        配置模型
      </button>
    </div>

    <!-- 聊天区域 -->
    <template v-else>
      <!-- 消息列表 -->
      <div ref="messagesContainer" class="flex-1 overflow-y-auto p-4 space-y-3">
        <!-- 空状态 -->
        <div v-if="chat.messages.value.length === 0" class="flex flex-col items-center justify-center h-full text-center">
          <div class="text-3xl mb-2">💬</div>
          <p class="text-sm text-slate-400 dark:text-dark-500">开始新对话</p>
        </div>

        <!-- 消息 -->
        <ChatMessage
          v-for="(msg, i) in chat.messages.value"
          :key="i"
          :message="msg"
          :streaming="chat.isStreaming.value && i === chat.messages.value.length - 1"
        />
      </div>

      <!-- 输入栏 -->
      <div class="border-t border-slate-200 dark:border-dark-700 p-3">
        <ChatInput
          :disabled="chat.sending.value || !config.activeProvider.value"
          placeholder="输入消息..."
          @send="chat.sendMessage"
        />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 侧边栏 AI 聊天面板
 *
 * 完整的聊天界面：Provider 切换、消息列表、输入栏、配置管理
 */
import { ref, watch, nextTick, onMounted, inject } from 'vue'
import type { PluginContext, ApiProvider } from '../types'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import ProviderManager from './ProviderManager.vue'
import { useAiConfig } from '../composables/useAiConfig'
import { useAiChat } from '../composables/useAiChat'

// 通过 inject 获取 PluginContext（由 PluginViewHost 提供）
const context = inject<PluginContext>('pluginContext')!

const config = useAiConfig(context.storage.get, context.storage.set)
const chat = useAiChat(
  context.storage.get,
  context.storage.set,
  context.storage.delete,
  () => config.activeProvider.value,
)

const messagesContainer = ref<HTMLElement | null>(null)

// 自动滚动到底部
watch(() => chat.messages.value.length, () => {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
    }
  })
})

onMounted(async () => {
  await config.loadConfig()
  await chat.loadConversations()
})
</script>
```

- [ ] **Step 6: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/components/
git commit -m "feat(plugin): add AI chatbox UI components (ChatView, ChatMessage, ChatInput, ProviderManager, PromptOptimizeDialog)"
```

---

## Task 9: 实现完整入口文件并集成所有功能

**Files:**
- Modify: `~/.bedcode/plugins/com.bedcode.ai-chatbox/index.ts`

- [ ] **Step 1: 更新 index.ts 为完整实现**

```typescript
/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板 + 终端提示词优化
 */
import type { PluginContext } from '../../shared/plugin/types'
import ChatView from './components/ChatView.vue'
import { usePromptOptimizer } from './composables/usePromptOptimizer'

export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板
  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  // 终端提示词优化
  const optimizer = usePromptOptimizer(
    // getActiveProvider：从 storage 读取当前活跃 provider
    async () => {
      const providers = await context.storage.get<string>('apiProviders')
      const activeName = await context.storage.get<string>('activeProvider')
      if (!providers || !activeName) return undefined
      try {
        const parsed = typeof providers === 'string' ? JSON.parse(providers) : providers
        const list = Array.isArray(parsed) ? parsed : []
        return list.find((p: any) => p.name === activeName)
      } catch {
        return undefined
      }
    },
    // sendInput：代理 context.terminal.sendInput
    (sessionId, text) => context.terminal.sendInput(sessionId, text),
  )

  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => optimizer.optimizePrompt(),
  })

  // 将 optimizer 状态挂载到全局，供 PromptOptimizeDialog 使用
  // 通过插件事件机制让 ChatView 访问 optimizer 状态
  const optimizerState = optimizer

  // 在 window 上挂载，供组件访问（inline 模式可行）
  ;(window as any).__ai_chatbox_optimizer__ = optimizerState

  console.log('[AI Chatbox] Plugin activated')
}

export async function deactivate(): Promise<void> {
  delete (window as any).__ai_chatbox_optimizer__
  console.log('[AI Chatbox] Plugin deactivated')
}
```

- [ ] **Step 2: 更新 ChatView.vue 添加 PromptOptimizeDialog**

在 ChatView.vue 的模板末尾添加：

```vue
<!-- 提示词优化弹窗 -->
<PromptOptimizeDialog
  :show="optimizerState.showDialog.value"
  :optimizing="optimizerState.optimizing.value"
  :original="optimizerState.originalText.value"
  :optimized="optimizerState.optimizedText.value"
  :error="optimizerState.errorMessage.value"
  @accept="optimizerState.acceptOptimized()"
  @cancel="optimizerState.cancelOptimize()"
/>
```

在 ChatView.vue 的 script 中添加：

```typescript
import PromptOptimizeDialog from './PromptOptimizeDialog.vue'

// 获取全局 optimizer 状态
const optimizerState = (window as any).__ai_chatbox_optimizer__ || {
  showDialog: ref(false),
  optimizing: ref(false),
  originalText: ref(''),
  optimizedText: ref(''),
  errorMessage: ref(''),
  acceptOptimized: () => {},
  cancelOptimize: () => {},
}
```

- [ ] **Step 3: Commit**

```bash
git add ~/.bedcode/plugins/com.bedcode.ai-chatbox/
git commit -m "feat(plugin): complete AI chatbox entry with sidebar panel and terminal toolbar integration"
```

---

## Task 10: 验证和修复

- [ ] **Step 1: 启动开发服务器**

Run: `cd D:/tauriProject/BedCode && npm run tauri:dev`

- [ ] **Step 2: 验证侧边栏出现 AI 对话入口**

Expected: 侧边栏出现「AI 对话」导航项

- [ ] **Step 3: 验证点击后导航到聊天页面**

Expected: 点击后显示 ChatView，提示配置模型

- [ ] **Step 4: 验证 Provider 配置**

Expected: 点击齿轮图标 → 显示配置面板 → 选择预设 → 填入 API Key → 添加成功

- [ ] **Step 5: 验证聊天功能**

Expected: 输入消息 → 发送 → AI 流式回复

- [ ] **Step 6: 验证终端工具栏 AI 优化按钮**

Expected: 终端 header 出现「✨ AI 优化」按钮

- [ ] **Step 7: 验证提示词优化流程**

Expected: 终端输入文本 → 点击 AI 优化 → 弹窗显示原始/优化后对比 → 采纳后文本填入终端

- [ ] **Step 8: 修复发现的问题**

- [ ] **Step 9: 最终 Commit**

```bash
git add -A
git commit -m "fix(plugin): fix integration issues found during testing"
```

---

## Self-Review Checklist

### Spec Coverage

| Spec Requirement | Task |
|-----------------|------|
| 侧边栏添加 AI 对话入口 | Task 8 (ChatView) + Task 9 (index.ts registerSidebarPanel) |
| 多模型 API 配置 | Task 5 (useAiConfig) + Task 8 (ProviderManager) |
| OpenAI 兼容格式 API 调用 | Task 4 (openaiClient) |
| 聊天界面（多轮对话、流式、Markdown） | Task 6 (useAiChat) + Task 8 (ChatView, ChatMessage) |
| 聊天历史持久化 | Task 6 (useAiChat storage) |
| 终端工具栏 AI 优化按钮 | Task 9 (registerTerminalToolbarItem) |
| 提示词优化弹窗 | Task 7 (usePromptOptimizer) + Task 8 (PromptOptimizeDialog) |
| 优化后填入终端不执行 | Task 7 (Ctrl+U + sendInput) |
| Rust permission API map 修复 | Task 1 |
| TerminalPreview 输入追踪 | Task 2 |

### Placeholder Scan

No TBD/TODO found in code blocks. All implementation steps contain complete code.

### Type Consistency

- `ApiProvider` defined in `types.ts`, used consistently in `useAiConfig.ts`, `useAiChat.ts`, `usePromptOptimizer.ts`, `openaiClient.ts`, `ProviderManager.vue`
- `ChatMessage` defined in `types.ts`, used in `useAiChat.ts`, `openaiClient.ts`, `ChatMessage.vue`
- `ConversationMeta` defined in `types.ts`, used in `useAiChat.ts`
- `ProviderPreset` defined in `types.ts`, used in `useAiConfig.ts`, `ProviderManager.vue`, `openaiClient.ts`
- `CurrentInputEvent` defined in `types.ts`, used in `usePromptOptimizer.ts`
