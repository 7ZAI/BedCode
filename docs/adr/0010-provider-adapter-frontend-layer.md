# 供应商差异处理放插件前端适配层（raw 流 + 前端解析），不扩展宿主 sseFormat

**Status**: accepted

## Context

AI Chatbox 要支持多家供应商协议（OpenAI 兼容 / Anthropic Messages / Gemini）与 DeepSeek 思考模式（`delta.reasoning_content`）。宿主 `http_fetch` 仅内置 `sseFormat:"openai"` 一种解析（提取 `choices[0].delta.content`），思考内容会被直接丢弃；流 chunk 经 Tauri 事件只到达 webview 前端，插件 Rust（WASM guest）收不到流事件。

## Decision

双端插件的供应商协议差异全部收敛在**前端 TS 适配层**：`ProviderAdapter { apiStyle, buildRequest, parseStreamEvent }` 注册表按供应商配置的 `apiStyle`（openai / anthropic / gemini / custom）动态分派；请求统一走 raw 模式（`sseFormat:""`），SSE 缓冲与增量解析由各 adapter 在前端完成（含 `reasoning_content` 提取与 usage 透传）。Rust 侧只留校验 + 透传请求 JSON 的薄命令。宿主与 SDK 零改动。

- `apiFormat` 字段重命名为 `apiStyle`（旧数据加载时映射，写入一律新键）
- `custom` 为私有网关逃生舱槽位：接口预留，本期不实现 UI
- 思考相关（`thinkingMode` / `reasoningEffort`）为插件级全局配置，各 adapter 自行映射为方言参数（DeepSeek `thinking.reasoning_effort`、Anthropic `budget_tokens`、Gemini `thinkingBudget`）

## Considered Options

- **扩展宿主 `sseFormat`**（加 anthropic/gemini 等）：宿主是跨插件公共底座，不该学习各家方言；且要求宿主随插件需求发版，违背"插件留下扩展空间"的初衷。宿主 openai 解析保留，对其他插件不受影响。
- **请求在 Rust 构建 / 响应在 TS 解析**：同一份协议知识拆两处、两边同步改，违背抽象初衷。

## Consequences

- 前端需要约 50 行 SSE 缓冲解析器（处理跨 chunk 断行）；事件语义（chunk/done/usage）由插件自管，宿主 `[DONE]` 与 usage 便利不再使用（raw 模式下宿主仅透传原始字节）。
- 思考过程（reasoning）随 assistant 消息新字段落盘 jsonl，历史对话可回看；重生成沿用 replaceLast 覆盖语义。
- 桌面/移动两端共用同一套 adapter 代码（各自复制同步，见 ADR-0011 的同步惯例）。
