# AI Chatbox Plugin (Desktop)

桌面端 AI 大模型对话插件：接入任意 OpenAI 兼容供应商，支持流式对话、多会话管理与代码高亮渲染。核心业务逻辑在 Rust WASM 层实现，TS 前端负责协议适配与 UI 渲染。

## 功能

- **多供应商**：预设 OpenAI / Anthropic / DeepSeek / 通义千问（Qwen）四家，另支持自定义 Base URL——Base URL、API Key、模型均可配置，支持 `/models` 拉取模型列表
- **流式对话**：SSE 流式输出，思考模式（default / enabled / disabled）与推理强度（low / high / max）可调；展示思考过程可开关
- **会话管理**：历史会话保存、恢复、删除；对话日志 JSONL 落盘，可追溯审计
- **限流重试**：供应商返回 429/503/529 等限流码时按指数退避自动重试（次数、初始等待、封顶均可配置）
- **代码渲染**：Markdown（marked）+ 代码高亮（highlight.js），6 种高亮主题（auto / light / dark / github-light / github-dark / dracula），字号与行距可调

## 使用

侧边栏「AI 对话」→ 在供应商设置中填入 API 地址 / 密钥并选择模型 → 新建对话开始提问；对话自动保存，可在会话列表切换 / 删除。

## 架构

> 📊 架构图：[architecture.html](./docs/architecture.html)

- **Rust WASM 层**：对话日志 JSONL 落盘（`store.rs`）、HTTP 请求透传（`client.rs`）、命令路由（`commands.rs`）、数据目录集中授权（宿主 `fs_auth` 弹窗）
- **TS 前端**：协议适配层 `src/adapters/`（openai / anthropic / gemini / custom 方言的请求构建与 SSE 解析）、对话 UI 与设置页
- **激活流程**：激活时宿主弹出目录授权（`fs_auth` 弹窗）→ 同意后持久化授权记录并初始化数据目录 → 激活成功；拒绝/超时 → 激活失败（Error 状态），重新启用可重试；首次启用需停用再启用一次完成 WASI 预打开挂载
- **数据目录**：固定为 `{home}/.bedcode/ai-chatbox/`（与 manifest `wasiPreopenDirs` 声明一致），由宿主经 WASI 预打开为 guest 侧 `/data`，Rust 端 `std::fs` 直读直写；对话文件落盘于 `/data/conversations/{id}.jsonl`（首行 meta + 逐行消息），索引 `/data/index.jsonl`

## 目录结构

```
ai-chatbox/
├── plugin.json          # 插件清单（权限、命令、侧边栏视图、配置项）
├── rust/
│   └── src/
│       ├── lib.rs       # WASM 入口 + 激活/数据目录授权
│       ├── client.rs    # HTTP 请求透传（协议适配在前端）
│       ├── commands.rs  # 命令路由
│       └── store.rs     # JSONL 对话日志落盘
├── scripts/
│   └── build.js         # 统一构建脚本（Vite + Cargo WASM + 复制产物）
├── src/
│   ├── index.ts         # 插件入口（activate/deactivate、UI 注册、i18n 注入）
│   ├── types.ts         # 类型与默认值（Provider/Message/Config）
│   ├── adapters/        # 多方言供应商协议（openai / anthropic / gemini / custom / registry / sse / usage / types / utils）
│   ├── components/      # ChatView / ChatInput / ChatMessage / ConversationList / ProviderConfigPage / ProviderForm / ModelListEditor / ProviderAvatar / ConfirmDialog
│   ├── composables/     # useAiChat（会话与流式） / useAiConfig（供应商） / usePluginConfig（全局配置）
│   ├── utils/           # highlight（高亮引擎 seam） / markdown（未闭合标记补偿） / providerIcons（预设品牌图标）
│   ├── i18n/            # 插件翻译表（zh-CN / en）
│   ├── assets/          # 供应商品牌 SVG
│   ├── __tests__/       # vitest 测试（适配层 / markdown / 图标 / 会话 / 配置）
│   └── dev-mock.ts      # dev-shell 命令 mock（真实宿主不注册）
└── vite.config.ts       # Vite 配置
```

## 构建

```bash
cd bedcode-desktop/plugins/ai-chatbox
node scripts/build.js
```

构建脚本串联：`vite build` → `cargo build`（WASM，Component Model 编码）→ 复制产物到 `src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/`。

> 产物目录（`**/src-tauri/resources/plugins/`）已加入 .gitignore，打包/运行前需先执行构建。

## 配置项

| 配置 | 类型 | 说明 |
|------|------|------|
| `thinkingMode` | string | default：跟随模型默认；enabled：强制开启思考；disabled：强制关闭 |
| `reasoningEffort` | string | 推理强度（low / high / max），仅思考模式为 enabled 时生效 |
| `showReasoning` | boolean | 是否展示思考过程块（思考内容仍随对话日志落盘） |
| `codeLineHeight` | number | 代码块行距（0.5-2.0，默认 1.6） |
| `codeFontSize` | number | 代码块字体大小（px，11-18，默认 13） |
| `codeTheme` | string | 代码高亮主题（auto / light / dark / github-light / github-dark / dracula），auto 跟随宿主深浅色 |
| `rateLimitMaxRetries` | number | 限流自动重试次数（0 = 关闭，最大 10，默认 3） |
| `rateLimitInitialDelayMs` | number | 限流重试初始等待（ms，默认 1000，之后指数退避翻倍） |
| `rateLimitMaxDelayMs` | number | 限流重试等待上限（ms，默认 30000） |

## 插件权限

| 权限 | 用途 |
|------|------|
| `storage` | 插件独立数据库 / 数据目录 |
| `fs:read` / `fs:write` | 对话日志落盘 |
| `network:http` | 调用模型供应商 API |
| `broadcast` | 状态变更广播 |
| `ui:sidebar` | 侧边栏「AI 对话」视图 |

## 命令（WASM invoke_command）

| 命令 | 用途 |
|------|------|
| `ai-chatbox.chat-stream` | 流式对话 |
| `ai-chatbox.chat-complete` | 非流式对话 |
| `ai-chatbox.fetch-models` | 拉取供应商模型列表 |
| `ai-chatbox.list-conversations` / `get-messages` | 查询会话与消息 |
| `ai-chatbox.save-conversation` / `save-message` | 保存会话 / 消息 |
| `ai-chatbox.delete-conversation` | 删除会话 |

## 测试

```bash
pnpm run test:run   # vitest run（适配层 / markdown 渲染 / 图标映射 / 会话 / 配置）
```
