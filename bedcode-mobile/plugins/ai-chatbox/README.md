# AI Chatbox Plugin (Mobile)

移动端 AI 大模型对话插件：与桌面端同 ID（`com.bedcode.ai-chatbox`）的对应实现，独立运行于手机端，支持多供应商流式对话与会话管理。协议适配在 TS 前端，Rust WASM 层负责日志落盘与目录授权。

## 功能

- **多供应商**：OpenAI 兼容 / Anthropic / Gemini 三种协议方言，Base URL、API Key、模型均可配置，支持 `/models` 拉取模型列表
- **供应商预设**：内置 DeepSeek / 通义千问 / OpenAI / Anthropic 4 个预设模板 + 自定义；预设品牌图标（`src/assets/providers/`），支持测试连接
- **流式对话**：SSE 流式输出，思考模式（default / enabled / disabled）与推理强度（low / high / max）可调；发送 / 停止 / 重新生成可幂等收尾
- **限流自动重试**：429 / 503 / 529 等过载码指数退避重试，次数与等待上限可配（输入区上方滑出条展示等待倒计时）
- **会话管理**：历史会话保存、恢复、重命名、删除（左侧抽屉式列表）；对话日志 JSONL 落盘（`{AppDownloadsDir}/ai-chatbox/`，卸载不清数据）
- **代码渲染**：Markdown + 代码高亮（Shiki），6 种高亮主题，字号与行距可调（设置弹层自绘滑块）

## 使用

底部导航「AI」页或工具箱「AI 对话」→ 首次使用先进设置添加供应商并选择模型，随后即可开始聊天。

## 架构

> 📊 架构图：[architecture.html](./docs/architecture.html)

- **Rust WASM 层**：HTTP 请求透传（`client.rs`，协议方言知识在前端）、JSONL 对话日志落盘（`store.rs`）、命令路由（`commands.rs`）、数据目录集中授权（宿主 `fs_auth` 弹窗）
- **TS 前端**：协议适配层 `src/adapters/`（openai / anthropic / gemini 方言的请求构建与 SSE 解析，`registry.ts` 按 `apiStyle` 分派）、对话 UI（`ChatView` + `ConversationList`）、供应商配置（`ProviderConfigPage` / `ProviderForm` / `ProviderAvatar` / `ModelListEditor`）、设置弹层（`PluginSettingsSheet`）
- **激活流程**：激活时宿主弹出目录授权 → 同意后初始化数据目录 → 激活成功；拒绝/超时 → 激活失败，重新启用可重试

## 目录结构

```
ai-chatbox/
├── plugin.json          # 插件清单（权限、命令、navTab / toolbox 视图、配置项）
├── rust/
│   └── src/
│       ├── lib.rs       # WASM 入口 + 激活/数据目录授权
│       ├── client.rs    # HTTP 请求透传（协议适配在前端）
│       ├── commands.rs  # 命令路由
│       └── store.rs     # JSONL 对话日志落盘
├── src/
│   ├── adapters/        # 多方言供应商协议（openai / anthropic / gemini + custom 槽位）
│   │                     #  与 registry / sse / types / usage / utils
│   ├── components/      # ChatView / ChatInput / ChatMessage / ConversationList
│   │                     #  / ModelListEditor / PluginSettingsSheet
│   │                     #  / ProviderAvatar / ProviderConfigPage / ProviderForm
│   ├── composables/     # useAiChat / useAiConfig / usePluginConfig
│   ├── utils/           # markdown 渲染、Shiki 代码高亮、providerIcons
│   ├── i18n/            # 插件翻译表（zh-CN / en，messages.ts 为 key schema）
│   ├── assets/providers/# 预设供应商品牌 SVG（deepseek / qwen / openai / anthropic）
│   ├── __tests__/       # vitest 单测（适配层 / 高亮 / 组合式 / SSE / 配置）
│   ├── index.ts         # 插件入口（activate/deactivate + i18n + dev-shell mock）
│   ├── types.ts         # 插件内部类型定义与配置默认值
│   └── dev-mock.ts      # dev-shell 命令 mock（生产构建自动排除）
└── vite.config.ts       # Vite 配置
```

## 构建

```bash
cd bedcode-mobile
node scripts/plugin-build.js --plugin com.bedcode.ai-chatbox
```

产物复制到 `src-tauri/resources/plugins/mobile/com.bedcode.ai-chatbox/`（进 APK 资源）。

## 配置项

| 配置 | 类型 | 说明 |
|------|------|------|
| `thinkingMode` | string | default：跟随模型默认；enabled：强制开启思考；disabled：强制关闭 |
| `reasoningEffort` | string | 推理强度（low / high / max），仅思考模式为 enabled 时生效 |
| `showReasoning` | boolean | 是否展示思考过程块（思考内容仍随对话日志落盘） |
| `codeLineHeight` | number | 代码块行距（0.5-2.0，默认 1.6） |
| `codeFontSize` | number | 代码块字体大小（px，11-18，默认 13） |
| `codeTheme` | string | 代码高亮主题（auto / light / dark / github-light / github-dark / dracula），auto 跟随宿主深浅色 |
| `rateLimitMaxRetries` | number | 限流自动重试次数（0 = 关闭自动重试，默认 3） |
| `rateLimitInitialDelayMs` | number | 限流重试初始等待（ms，默认 1000，之后按指数退避翻倍） |
| `rateLimitMaxDelayMs` | number | 限流重试等待上限（ms，指数退避封顶值，默认 30000） |

## 插件权限

| 权限 | 用途 |
|------|------|
| `storage` | 插件独立数据库 / 数据目录 |
| `fs:read` / `fs:write` | 对话日志落盘 |
| `network:http` | 调用模型供应商 API |
| `ui:navtab` | 底部导航「AI」入口 |
| `ui:toolbox` | 工具箱「AI 对话」入口 |

## 命令（WASM invoke_command）

与桌面端一致：`ai-chatbox.chat-stream` / `chat-complete` / `fetch-models` / `list-conversations` / `get-messages` / `save-conversation` / `save-message` / `delete-conversation`。

## 测试

```bash
npm run test:run   # vitest run
```

覆盖：适配层（openai / anthropic / gemini / registry）、SSE 缓冲、markdown 未闭合标记补偿、Shiki 高亮、useAiChat / useAiConfig / usePluginConfig。
