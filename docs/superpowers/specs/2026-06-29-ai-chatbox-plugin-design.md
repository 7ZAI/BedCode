# BedCode AI Chatbox 插件设计

> 桌面端内置 AI 对话插件，提供侧边栏聊天面板、多模型 API 配置、终端提示词优化功能。

## 目标

- 在侧边栏添加「AI 对话」入口，点击后导航到完整聊天页面
- 支持通过插件配置管理多个 OpenAI 兼容格式的 API Key（名称、密钥、Base URL、模型名）
- 在终端工具栏添加「AI 优化」按钮，将当前终端输入内容发送到 AI 进行提示词优化，结果以弹窗确认后填入输入框（不执行）
- 聊天历史持久化到插件 storage

## 架构

本插件完全在 BedCode 插件系统框架内实现，作为一个内置插件放置于 `~/.bedcode/plugins/com.bedcode.ai-chatbox/`。

```
┌─────────────────────────────────────────────────────────────┐
│                      Plugin Runtime                          │
│                                                             │
│  plugin.json ──► PluginLoader ──► PluginContext             │
│       │                │              │                      │
│       ▼                ▼              ▼                      │
│  Contributes      activate()    context.ui / storage        │
│  (views/config)   注册 UI       (注册面板、工具栏、存储)    │
└─────────────────────────────────────────────────────────────┘
```

**核心原则：**

- **声明式注册**：通过 `plugin.json` 的 `contributes` 声明视图和配置 schema
- **PluginContext 唯一通道**：插件只通过 context API 访问宿主能力
- **OpenAI 兼容格式**：所有模型 API 调用统一走 OpenAI chat completion 格式，通过不同 base URL 区分提供商
- **配置表单自动生成**：利用 `contributes.configuration` 让 PluginConfigView 自动生成 API Key 配置表单

---

## 1. plugin.json

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
          "description": "JSON array of API provider configs. Each provider: { name, apiKey, baseUrl, model }",
          "default": "[]"
        },
        "activeProvider": {
          "type": "string",
          "title": "Active Provider Name",
          "description": "Name of the currently active API provider",
          "default": ""
        }
      }
    }
  }
}
```

### permissions 说明

| 权限 | 用途 |
|------|------|
| `ui:sidebar` | 注册侧边栏聊天面板 |
| `ui:input` | 注册终端工具栏「AI 优化」按钮 |
| `storage` | 持久化聊天历史和配置 |
| `terminal:input` | AI 优化后填入终端输入（通过 sendInput） |
| `session:read` | 获取当前活跃会话 ID |

### configuration 设计

由于 `contributes.configuration` 的 `ConfigProperty` 类型只支持 `string | number | boolean`，无法直接表达「多个 API Key 配置」这样的复杂数据结构。因此采用以下策略：

- `apiProviders`：存储为 JSON 字符串，值为 `[{ name, apiKey, baseUrl, model }]` 数组
- `activeProvider`：当前活跃的 provider 名称

**在 ChatView 内部提供更友好的配置管理界面**，允许用户添加/删除/编辑多个 API provider，保存时将数组序列化为 JSON 字符串写入 storage。PluginConfigView 作为基础配置入口，ChatView 内部作为高级配置入口。

---

## 2. 插件目录结构

```
~/.bedcode/plugins/com.bedcode.ai-chatbox/
├── plugin.json
├── index.ts                     # 入口文件：activate / deactivate
├── composables/
│   ├── useAiChat.ts             # 聊天核心逻辑：发送消息、流式接收、历史管理
│   ├── useAiConfig.ts           # API provider 配置管理：增删改查、活跃切换
│   └── usePromptOptimizer.ts    # 终端提示词优化逻辑
├── components/
│   ├── ChatView.vue             # 侧边栏聊天面板（完整聊天界面）
│   ├── ChatMessage.vue          # 单条消息渲染（支持 Markdown）
│   ├── ChatInput.vue            # 消息输入栏
│   ├── ProviderManager.vue      # API provider 配置管理（在 ChatView 内嵌）
│   └── PromptOptimizeDialog.vue # 提示词优化弹窗
├── services/
│   └── openaiClient.ts          # OpenAI 兼容格式 API 客户端（fetch 实现）
│   └── markdownRenderer.ts      # Markdown → HTML 渲染工具
└── types.ts                     # 插件内部类型定义
```

---

## 3. 核心功能设计

### 3.1 AI 对话面板 (ChatView)

**注册方式：** `context.ui.registerSidebarPanel({ id: 'ai-chatbox.sidebar', title: 'AI 对话', component: ChatView })`

**路由：** `/plugin/sidebar/com.bedcode.ai-chatbox/ai-chatbox.sidebar`

**功能：**

- 多轮对话：消息列表 + 输入框，支持上下文连续对话
- 流式输出：AI 回复使用 streaming 逐字显示
- Markdown 渲染：AI 回复中的代码块、列表、表格等正确渲染
- 新建对话：每次新建清空历史，开始全新对话
- 对话列表：左侧显示历史对话列表，点击切换
- Provider 切换：顶部下拉菜单切换当前活跃的 API provider
- Provider 管理：齿轮图标打开 ProviderManager 内嵌面板

**布局：**

```
┌─────────────────────────────────────────────────┐
│  [Provider: DeepSeek ▼]  [⚙]  [+ 新对话]      │
├─────────────────────────────────────────────────┤
│                                                 │
│  用户:帮我写一个 Rust 的 HTTP 服务器            │
│                                                 │
│  AI:好的，这是一个使用 Tokio 的简单 HTTP...     │
│  ```rust                                        │
│  use tokio::net::TcpListener;                   │
│  ...                                            │
│  ```                                            │
│                                                 │
├─────────────────────────────────────────────────┤
│  [输入消息...]                    [发送 ➤]      │
└─────────────────────────────────────────────────┘
```

### 3.2 API Provider 配置管理 (ProviderManager)

**内嵌于 ChatView**，通过齿轮图标展开/收起。

**数据模型：**

```typescript
interface ApiProvider {
  name: string        // 显示名称，如 "DeepSeek"、"OpenAI"
  apiKey: string      // API 密钥
  baseUrl: string     // API base URL，如 "https://api.deepseek.com/v1"
  model: string       // 模型名，如 "deepseek-chat"
}
```

**操作：**

- 添加新 provider：填写名称、API Key、Base URL、模型名
- 编辑已有 provider
- 删除 provider
- 切换活跃 provider（高亮显示当前活跃）
- 预设模板：提供常见 provider 的 Base URL 和模型名预设（DeepSeek、通义千问、OpenAI、Moonshot 等）

**存储：**

- `context.storage.set('apiProviders', JSON.stringify(providers))`
- `context.storage.set('activeProvider', activeName)`
- `context.storage.set('config', configJson)` — 与 PluginConfigView 同步（contributes.configuration key "config"）

### 3.3 终端提示词优化 (PromptOptimizer)

**注册方式：** `context.ui.registerTerminalToolbarItem({ id: 'ai-optimize-prompt', label: 'AI 优化', icon: '✨', onClick: optimizePrompt })`

**交互流程：**

```
1. 用户在终端中输入提示词（未按回车）
2. 点击终端工具栏的「✨ AI 优化」按钮
3. 插件通过事件请求获取当前终端输入内容：
   context.events.emit('ai-chatbox:getCurrentInput')
   → 宿主 TerminalPreview 返回 { sessionId, text }
4. 构造优化 prompt："请优化以下提示词，使其更清晰、更具体、更容易让 AI 理解。保持原始意图不变，但改进表达方式：\n\n{用户原始输入}"
5. 调用 OpenAI 兼容 API 发送请求
6. 弹出 PromptOptimizeDialog 显示：
   - 原始提示词
   - 优化后的提示词
   - [采纳] [取消] 按钮
7. 用户点击「采纳」：
   - 发送 Ctrl+U（清除当前行）+ 优化后的提示词
   - context.terminal.sendInput(sessionId, '\x15' + optimizedText)
   - 文本出现在终端但未执行（未发送换行符）
8. 用户点击「取消」：关闭弹窗，不做任何操作
```

**PromptOptimizeDialog 设计：**

```
┌─────────────────────────────────────────────────┐
│  AI 提示词优化                                  │
├─────────────────────────────────────────────────┤
│  原始提示词:                                    │
│  ┌─────────────────────────────────────────┐    │
│  │ 帮我写一个 Rust 服务器                    │    │
│  └─────────────────────────────────────────┘    │
│                                                 │
│  优化后提示词:                                  │
│  ┌─────────────────────────────────────────┐    │
│  │ 请使用 Rust 和 Tokio 异步框架编写一个    │    │
│  │ HTTP 服务器，支持路由分发和 JSON 响应...  │    │
│  └─────────────────────────────────────────┘    │
│                                                 │
│  [采纳并填入终端]            [取消]             │
└─────────────────────────────────────────────────┘
```

---

## 4. OpenAI 兼容 API 客户端

### 4.1 核心实现

使用浏览器 `fetch` API 直接调用 OpenAI chat completion endpoint，无需后端代理。

```typescript
class OpenAiClient {
  private provider: ApiProvider

  constructor(provider: ApiProvider) {
    this.provider = provider
  }

  /** 发送聊天请求（流式） */
  async chatStream(
    messages: ChatMessage[],
    onChunk: (text: string) => void,
    onDone: () => void,
    onError: (error: Error) => void,
  ): Promise<void> {
    const url = `${this.provider.baseUrl}/chat/completions`
    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${this.provider.apiKey}`,
      },
      body: JSON.stringify({
        model: this.provider.model,
        messages,
        stream: true,
      }),
    })

    // 流式读取 SSE
    const reader = response.body!.getReader()
    const decoder = new TextDecoder()
    // ... SSE 解析逻辑
  }

  /** 发送聊天请求（非流式，用于提示词优化） */
  async chat(messages: ChatMessage[]): Promise<string> {
    const url = `${this.provider.baseUrl}/chat/completions`
    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${this.provider.apiKey}`,
      },
      body: JSON.stringify({
        model: this.provider.model,
        messages,
        stream: false,
      }),
    })
    const data = await response.json()
    return data.choices[0].message.content
  }
}
```

### 4.2 预设 Provider 模板

```typescript
const PROVIDER_PRESETS: ApiProvider[] = [
  { name: 'DeepSeek', apiKey: '', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { name: 'OpenAI', apiKey: '', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { name: '通义千问', apiKey: '', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-turbo' },
  { name: 'Moonshot', apiKey: '', baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  { name: '智谱', apiKey: '', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4-flash' },
]
```

用户添加 provider 时可从预设中选择模板，只需填入 API Key 即可。

---

## 5. 聊天历史持久化

### 5.1 存储结构

使用 `context.storage` 存储聊天数据：

- **对话列表**：`storage.set('conversations', JSON.stringify(conversationList))`
- **对话消息**：`storage.set('conv:${convId}', JSON.stringify(messages))`

```typescript
interface ConversationMeta {
  id: string          // UUID
  title: string       // 首条消息摘要或自动生成
  createdAt: string   // ISO 8601
  updatedAt: string   // ISO 8601
  providerName: string // 使用的 provider
}

interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string   // ISO 8601
}
```

### 5.2 读写策略

- 列表操作：读取 `conversations` key，解析 JSON
- 加载对话：读取 `conv:${id}` key，解析消息列表
- 保存：每次发送/接收消息后即时写入（storage 是即时写入的）
- 删除对话：删除 `conv:${id}` + 更新 `conversations` 列表

---

## 6. 入口文件设计

```typescript
// index.ts
import type { PluginContext } from './types'
import ChatView from './components/ChatView.vue'

export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板
  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => optimizePrompt(context),
  })
}

export async function deactivate(): Promise<void> {
  // Disposable 由 PluginLoader 自动清理
}

/** 终端提示词优化主逻辑 */
async function optimizePrompt(context: PluginContext): Promise<void> {
  // 1. 获取当前终端输入
  // 2. 获取当前活跃会话
  // 3. 调用 AI API 优化
  // 4. 弹出确认弹窗
  // 5. 采纳后通过 terminal.sendInput 填入
}
```

---

## 7. 与宿主的交互边界

### 插件需要的宿主能力

| API | 用途 |
|-----|------|
| `context.ui.registerSidebarPanel` | 注册侧边栏聊天面板 |
| `context.ui.registerTerminalToolbarItem` | 注册终端工具栏优化按钮 |
| `context.terminal.sendInput` | 将优化后的提示词填入终端输入（Ctrl+U + 优化文本） |
| `context.session.list` | 获取会话列表，确定当前活跃会话 ID |
| `context.storage.get/set/delete` | 持久化配置和聊天历史 |
| `context.events.emit/on` | 与宿主组件通信（获取当前终端输入） |

### 插件不需要的宿主能力

- 不需要 `http.registerEndpoint`（直接使用 fetch）
- 不需要 `context.commands`（不需要命令面板）
- 不需要 `ui:statusbar`、`ui:toolbox`
- 不需要 `session:write`

---

## 8. 关键实现细节

### 8.1 获取终端当前输入

**问题**：桌面端终端是 xterm.js 渲染的 PTY 终端，用户输入直接发送到 PTY 进程，不存在独立的"输入框"可读取。`terminal.onInput` 事件仅在输入提交到 PTY 时触发，无法获取用户正在输入但尚未提交的文本。

**解决方案**：通过插件事件机制，让宿主组件提供当前终端输入缓冲区内容。

具体实现：
1. 在 `TerminalPreview.vue` 中，监听 xterm.js 的 `onData` 事件，追踪当前行输入缓冲区
2. 当插件触发 `ai-chatbox:getCurrentInput` 事件时，宿主组件通过 `ai-chatbox:currentInput` 事件返回当前输入
3. 插件侧使用 `context.events.emit` 请求 + `context.events.on` 接收

**宿主侧改动（TerminalPreview.vue）**：

```typescript
// 追踪当前行输入（简化版：记录从最后一个换行符之后的内容）
let currentLineBuffer = ''
terminal.onData((data) => {
  if (data === '\r') {
    currentLineBuffer = ''
  } else if (data === '\x7f' || data === '\b') {
    currentLineBuffer = currentLineBuffer.slice(0, -1)
  } else if (data.length === 1 && data.charCodeAt(0) >= 32) {
    currentLineBuffer += data
  }
  // 其他控制序列忽略（方向键历史等复杂场景暂不处理）
})

// 监听插件请求
const registry = getPluginRegistry()
registry.on('ai-chatbox:getCurrentInput', () => {
  registry.emit('ai-chatbox:currentInput', { sessionId, text: currentLineBuffer })
})
```

**MVP 约束**：此方案只能追踪简单的逐字符输入，无法处理方向键历史回溯、行编辑快捷键（Ctrl+A/E/U/K 等）等复杂场景。对于 MVP 阶段足够使用。

### 8.2 将优化后的文本填入终端

`context.terminal.sendInput(sessionId, text)` 通过 `write_input` 写入 PTY，**不追加换行符**。因此调用 `sendInput(sessionId, optimizedText)` 会在终端显示文本但不执行（等于用户手打但没按回车）。

**但有一个问题**：如果终端当前行已有部分输入，`sendInput` 会在光标位置追加文本，而非替换。需要先清除当前行再填入。

**解决方案**：发送清除当前行 + 填入优化文本的控制序列：

```typescript
// \x15 = Ctrl+U（清除当前行），然后填入优化文本
context.terminal.sendInput(sessionId, '\x15' + optimizedText)
```

Ctrl+U 在大多数 shell（bash/zsh）中会清除当前行输入，然后 `optimizedText` 会出现在新的空行上。

### 8.3 流式输出 SSE 解析

OpenAI streaming 返回 Server-Sent Events 格式：

```
data: {"id":"...","choices":[{"delta":{"content":"你"}}]}
data: {"id":"...","choices":[{"delta":{"content":"好"}}]}
data: [DONE]
```

需要逐行解析 `data:` 行，提取 `delta.content`，拼接为完整回复。

### 8.4 Markdown 渲染

AI 回复中的 Markdown 内容需要渲染为 HTML。使用轻量级方案：

- 引入 `marked` 库（CDN 或打包进插件）
- 代码块使用 `highlight.js` 或简单 `<pre><code>` 渲染
- 考虑插件 inline 模式下的依赖管理

**方案选择：** 使用 `marked` + 内置代码块样式，不引入 `highlight.js`（减少体积）。代码块以 `<pre><code>` 渲染，配合 BedCode 已有的深色主题样式。

---

## 9. i18n

插件内部使用中文界面（与 BedCode 主应用一致），技术术语保留英文。

**需要 i18n 的内容：**

| key | zh-CN | en |
|-----|-------|----|
| `ai-chatbox.title` | AI 对话 | AI Chat |
| `ai-chatbox.newConversation` | 新对话 | New Chat |
| `ai-chatbox.optimizePrompt` | AI 优化 | AI Optimize |
| `ai-chatbox.originalPrompt` | 原始提示词 | Original Prompt |
| `ai-chatbox.optimizedPrompt` | 优化后提示词 | Optimized Prompt |
| `ai-chatbox.acceptAndFill` | 采纳并填入终端 | Accept & Fill Terminal |
| `ai-chatbox.cancel` | 取消 | Cancel |
| `ai-chatbox.providerSettings` | 模型配置 | Model Settings |
| `ai-chatbox.addProvider` | 添加模型 | Add Model |
| `ai-chatbox.apiKey` | API Key | API Key |
| `ai-chatbox.baseUrl` | Base URL | Base URL |
| `ai-chatbox.model` | 模型 | Model |
| `ai-chatbox.noProvider` | 请先配置 AI 模型 | Please configure an AI model first |
| `ai-chatbox.sendFailed` | 发送失败 | Send failed |
| `ai-chatbox.noInput` | 终端无输入内容 | No input in terminal |

由于插件不运行在主应用的 i18n 体系中，插件内部自行管理国际化字符串（硬编码中文为主，后续可扩展为英文支持）。

---

## 10. 错误处理

| 场景 | 处理 |
|------|------|
| API Key 未配置 | ChatView 显示配置引导提示 |
| API 请求失败 | 显示错误消息，不中断对话 |
| 流式连接中断 | 标记回复为不完整，显示「连接中断」 |
| 终端无输入 | 提示「终端无输入内容」 |
| 优化请求失败 | 弹窗显示错误，不填入终端 |
| Storage 读写失败 | 降级为内存存储，不丢失当前对话 |

---

## 11. 新增文件清单

| 文件 | 职责 |
|------|------|
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/plugin.json` | 插件描述文件 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/index.ts` | 入口文件 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/types.ts` | 内部类型定义 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/useAiChat.ts` | 聊天核心逻辑 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/useAiConfig.ts` | API 配置管理 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/composables/usePromptOptimizer.ts` | 提示词优化 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ChatView.vue` | 侧边栏聊天面板 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ChatMessage.vue` | 消息渲染组件 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ChatInput.vue` | 输入栏组件 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/ProviderManager.vue` | 配置管理组件 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/components/PromptOptimizeDialog.vue` | 优化弹窗组件 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts` | API 客户端 |
| `~/.bedcode/plugins/com.bedcode.ai-chatbox/services/markdownRenderer.ts` | Markdown 渲染 |

## 12. 修改文件清单

| 文件 | 变更 |
|------|------|
| `src-tauri/src/desktop/plugin/permission.rs` | `PERMISSION_API_MAP` 中 `ui:input` 补充 `ui.registerTerminalToolbarItem` |
| `src/modules/desktop/components/TerminalPreview.vue` | 添加当前行输入追踪 + 插件事件响应（约 20 行） |

**注意：** 查看当前 permission.rs 代码，`PERMISSION_UI_INPUT` 映射到 `["ui.registerInputExtension"]`，不含 `ui.registerTerminalToolbarItem`。需要补充。

---

## 13. 不变文件

- Rust PluginHost / PluginLoader / PluginStorage — 无需改动
- 前端 PluginLoader / PluginContext / PluginRegistryClass — 无需改动
- PluginTerminalToolbar.vue — 已实现，自动渲染注册的 toolbar items
- PluginViewHost.vue — 已实现，自动渲染 sidebar views
- Sidebar.vue — 已实现，自动显示插件导航链接
