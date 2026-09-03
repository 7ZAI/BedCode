# AI Chatbox 流式渲染管线 + 协议适配层规格（双端）

Status: ready-for-agent

> 本规格由 grilling 会话全程决策汇编而成（grill-with-docs + domain-modeling），术语遵循 `CONTEXT.md` 词汇表，架构决策见 `docs/adr/0010-provider-adapter-frontend-layer.md` 与 `docs/adr/0011-dual-highlight-engine-comparison.md`。实现者无需再做重大决策。前序规格：`.scratch/ai-chatbox-rebuild/spec.md`（已完成）、`.scratch/ai-chatbox-config-ui/spec.md`（ready-for-agent）。

## Problem Statement

用户在 AI Chatbox 插件（桌面端为主诉，移动端同构）中遇到的体验问题：

1. **流式回复（打字机效果）卡顿**：每个 chunk 触发一次全量 Markdown 重解析 + 全量消毒 + 全量代码重高亮 + DOM 重建，长回复 + 多代码块时 O(n²)，主线程掉帧，打字机节奏不跟手。
2. **流式过程中布局跳动**：模型输出代码块 fence（```` ``` ````）未闭合时，解析器把其后全部文本吞进代码块，语言标签/复制按钮头部压满屏，直到闭合 fence 到达才恢复——没有未闭合标记补偿。
3. **代码高亮保真度不足**：hljs 正则启发式对现代语法（TS/泛型/JSX）常有误判，达不到 IDE 级；移动端宿主已用 Shiki 而插件没用。
4. **只支持 OpenAI 兼容方言**：供应商请求/响应形状写死在宿主 `sseFormat:"openai"` 解析（宿主还会丢弃 DeepSeek 思考模式的 `reasoning_content`），Anthropic / Gemini 方言与私有网关无法接入，换供应商无扩展空间。

## Solution

- **流式回复渲染管线重构**（双端同步）：rAF 节流 flush（每帧至多一次渲染，`done` 终态立即 flush）+ 未闭合标记补偿 + 延迟高亮（只高亮已闭合代码块）+ 代码块头部闭合后注入。保持 marked 全量解析，增量解析方案预留。
- **双引擎高亮对比实验**（ADR-0011）：桌面端保持 hljs（现有 CSS token 配色），移动端改用 Shiki（复制宿主单例接入模式）；渲染管线以高亮引擎 seam 隔离，两端效果对比为后续统一决策提供数据。
- **协议适配层**（ADR-0010）：供应商差异全部收敛到前端 TS 适配层——`ProviderAdapter` 注册表按供应商配置的「协议方言」动态分派，统一 raw 流（宿主 `sseFormat:""`）+ 前端 SSE 缓冲解析；内置 openai / anthropic / gemini 三种方言，custom 为逃生舱槽位。宿主与 SDK 零改动。
- **思考模式支持**：openai 方言提取 `reasoning_content`，以可折叠「思考过程」块展示（`showReasoning` 可关），`reasoning` 随对话日志落盘、重生成沿用既有覆盖语义。
- **插件级配置**：manifest 新增 `contributes.configuration` 三项全局配置——`thinkingMode`（default/enabled/disabled）、`reasoningEffort`（low/high/max）、`showReasoning`（bool），各方言适配器自行映射为对应 API 参数。

## User Stories

1. 作为用户，我希望流式回复期间界面保持流畅不卡顿，这样即使长回复+多个代码块也能逐字顺畅输出
2. 作为用户，我希望代码块开始输出时文本停留在代码块容器内而不是整段被吞成代码块，这样流式过程中布局不跳动
3. 作为用户，我希望代码块未闭合时不显示语言标签/复制按钮，这样按钮不会盖住正在输出的代码
4. 作为用户，我希望代码块闭合后立即获得语法高亮与复制按钮，这样最终效果与主流 AI 对话一致
5. 作为用户，我希望代码高亮在流式过程中不拖慢渲染，这样"打字机效果"与高亮质量互不拖累
6. 作为用户，我希望桌面端代码高亮保持当前低饱和配色风格，这样与宿主主题协调
7. 作为用户，我希望移动端代码高亮达到 IDE 级保真（Shiki），这样移动端阅读代码体验与宿主文件查看器一致
8. 作为用户，我希望 DeepSeek 思考模型的思考过程以可折叠块展示（默认展开），这样我能看到模型"正在思考"而不被打断正文阅读
9. 作为用户，我希望在插件配置中关闭思考过程展示，这样只关心结论时界面更干净
10. 作为用户，我希望在插件配置中选择思考模式（跟随模型默认/强制开启/强制关闭）与推理强度（low/high/max），这样按场景控制思考深度与响应速度
11. 作为用户，我希望历史对话重开时仍能看到当时的思考过程，这样思考内容与正文一样可回溯
12. 作为用户，我希望停止生成/重新生成时思考过程与正文一起截断或覆盖，这样不会出现新旧思考内容混杂
13. 作为用户，我希望切换到 Anthropic 官方供应商（或 DeepSeek 的 Anthropic 兼容端点）后无需改代码即可对话，这样更多模型服务可直接接入
14. 作为用户，我希望接入 Gemini 供应商后流式对话正常解析（其事件形状与 OpenAI 不同），这样不局限于 OpenAI 兼容服务
15. 作为用户，我希望供应商配置的协议方言有明确默认值（旧配置自动按 openai 处理），这样升级插件后现有供应商配置无需重新填写
16. 作为供应商接入者，我希望新增一家供应商只需按适配器接口实现请求构建与流解析并注册，这样扩展协议方言无需改动宿主或 Rust
17. 作为供应商接入者，我希望思考类全局配置由各适配器自行映射为方言参数（DeepSeek `thinking.reasoning_effort` / Anthropic `budget_tokens` / Gemini `thinkingBudget`），这样同一份全局配置适配不同 API 语义
18. 作为供应商接入者，我希望自定义（custom）协议方言在接口层预留槽位，这样私有网关将来有接入位而不阻塞本期实现
19. 作为开发者，我希望渲染管线与高亮引擎解耦（引擎 seam），这样双端各自换引擎不触碰管线逻辑
20. 作为开发者，我希望 token 用量在流结束时照常显示，这样流解析改造不回归既有 usage 展示

## Implementation Decisions

### 渲染管线（双端同步，宿主零改动）

- **rAF 节流 flush**：组合层累积 chunk 到缓冲区，rAF 驱动批量写回消息内容（每帧至多一次）；流 `done` 时立即 flush 终态；停止/失败沿用既有截断落盘语义。
- **保持 marked 全量解析**：节流后每帧一次全量解析（亚毫秒~几毫秒级），不换增量解析器；**增量解析（markdown-it-incremental 类）为预留升级点**，跑测出现解析瓶颈后再引入，不在本期。
- **未闭合标记补偿**：解析前对累积文本做纯函数修补——奇数个 ` ``` ` fence 追加闭合；行尾未闭合的行内 `` ` `` 补上。决策逻辑独立成纯函数（测试接缝 2）。
- **延迟高亮**：只对已闭合代码块执行高亮；未闭合块渲染为纯文本 pre（fence 补偿保证其位于代码块容器内）；闭合后下一帧自然获得高亮。
- **代码块头部延迟注入**：语言标签/复制按钮仅在块闭合后注入（幂等）；未闭合块不注入。
- **DOMPurify 每帧保持**：LLM 输出不可信是硬约束，不跳过、不缓存。
- **光标保持现状**（独立 pulse span），不纳入本期。

### 高亮引擎 seam（ADR-0011）

- 定义 `HighlightEngine` 接口（对已闭合块产出高亮 HTML / 就地高亮），渲染管线只依赖接口。
- 桌面端注入 hljs 实现：沿用现有同步 `highlightElement` 路径与 CSS token 双套配色。
- 移动端注入 Shiki 实现：复制宿主接入模式（`createHighlighterCore` + oniguruma WASM 引擎 + 静态语言导入 + 懒加载单例），按深浅色模式切换两套内置主题（不映射 CSS token）。
- 两端各自带一份引擎实现；移动端 Shiki 实例为独立 bundle（成本已知）。宿主 Shiki 封装为插件 API 属后续项，不在本期。

### 协议适配层（ADR-0010）

- 纯 TS 接口：`ProviderAdapter { apiStyle, buildRequest(provider, messages, streamId), parseStreamEvent(line) }`；注册表按供应商「协议方言」字段动态分派。
- 统一 raw 流：`buildRequest` 产出 `{ method, url, headers, body, stream: true, streamEvent, sseFormat: "" }`；前端 SSE 缓冲器按行切分事件（处理跨 chunk 断行），产出 `{ chunk?, reasoning?, usage?, done? }`。
- 内置方言：`openai`（含 `stream_options.include_usage` 与 `delta.reasoning_content` 提取）、`anthropic`（`x-api-key` + `anthropic-version` 头、system 独立字段、`content_block_delta.delta.text`、`message_delta.usage`）、`gemini`（`candidates[0].content.parts[0].text`、`usageMetadata`）；`custom` 为逃生舱槽位（接口预留，不实现 UI）。
- **字段重命名**：供应商配置 `apiFormat` → `apiStyle`；加载时映射旧键（旧数据照常读入），写入一律新键。
- **Rust 变薄**：删除各供应商请求构建特化，保留校验 + 请求 JSON 透传 `http_fetch` 的薄命令；`chat-complete`（测试连接）与 `fetch-models` 同样走适配层构建。
- **思考参数映射**：插件级配置三项为全局语义，各方言适配器自行映射——openai 方言仅当 `thinkingMode ≠ default` 时写入 `thinking` 对象（`type` + `reasoning_effort`），anthropic/gemini 方言本期映射预算类参数（如 `budget_tokens` / `thinkingBudget`），无法映射的方言忽略请求侧参数。

### 思考模式与数据模型

- `ChatMessage` 新增可选 `reasoning` 字段（思考过程全文）；流式期间随 chunk 累积，`showReasoning` 为 false 时仅提取不展示。
- 落盘：assistant 消息 `reasoning` 随对话日志 JSONL 逐行写入；`save-message` 命令参数新增 `reasoning`；重生成沿用 replaceLast 覆盖语义（正文与思考一并覆盖）。
- UI：可折叠「思考过程」块，独立于正文的次级样式；流式期间默认展开，`showReasoning=false` 时整体不渲染。

### 插件级配置

- manifest 新增 `contributes.configuration`（双端），storage 统一键 `config`，宿主配置页按 schema 自动渲染。
- 配置项与默认值：`thinkingMode`（enum: default/enabled/disabled，默认 default=不传参跟随模型）、`reasoningEffort`（enum: low/high/max，默认 high，仅 thinkingMode=enabled 时生效）、`showReasoning`（boolean，默认 true）。
- 读取侧合并默认值（宿主配置页保存的值可能缺项）。

### 双端同步惯例

- 渲染核心、适配层、SSE 缓冲器、配置读写均为引擎无关纯模块，按现有惯例两端各自复制同步（不抽共享包）；差异仅在高亮引擎 adapter 与主题处理。

## Testing Decisions

- **好测试的定义**：只测外部行为（输入文本/事件 → 输出 chunk/reasoning/usage/补全后的文本），不测内部实现细节；纯函数接缝全部无框架依赖，vitest 直接覆盖。
- **接缝 1 — 协议适配层**（新，最高价值）：覆盖各方言请求形状（url/headers/body 断言）、SSE 缓冲跨 chunk 断行、`reasoning_content` 提取、usage 透传、`[DONE]` 终结、thinking 配置→方言参数映射、`apiStyle` 缺失默认 openai。
- **接缝 2 — Markdown 预处理**（新）：fence 补偿（奇数 fence 补闭合、行尾未闭合行内码补上）、闭合块状态检测（哪些块可高亮）、补偿幂等性。
- **接缝 3 — 组合层 composable**（已有，扩展）：沿用 `mockContext` 先例（`useAiChat.test.ts` / `useAiConfig.test.ts` / `providerIcons.test.ts`），fake timers 验证 rAF 节流 flush 语义、`reasoning` 落盘与重生成覆盖、插件级配置读写与默认值合并。
- **移动端 Shiki adapter**：node 环境集成测试验证契约（给定代码+语言能产出高亮 HTML，未支持语言降级 plaintext）；与宿主 `useCodeHighlight` 行为对齐。
- **不做组件级 DOM 测试**（不引入 @vue/test-utils/jsdom 新基建）：header 注入/思考块展开/高亮着色等 DOM 布线留手工验证 + 双端对比实验（ADR-0011 验收方式）；其决策逻辑已被接缝 2 覆盖。
- **双端各自运行测试**：`npm run test:run`（vitest run，禁止 watch 模式）于两个插件目录分别执行。

## Out of Scope

- 增量 Markdown 解析器（markdown-it-incremental 类）：预留升级点，跑测确认瓶颈后再评估
- 宿主 Shiki 封装为插件 API（`context` 高亮能力）：对比实验结论出来、统一引擎后单独立项
- custom 协议方言的配置 UI 与请求模板编辑：仅接口槽位
- 数学公式（KaTeX/MathJax）、图表（Mermaid）、富交互组件（Claude Artifacts 类）
- 光标样式改版（保持现有 pulse span）
- 宿主 `http_fetch` SSE 解析改造、其他插件行为
- 供应商配置迁移工具化：仅加载时字段映射，无独立迁移脚本

## Further Notes

- 依据的官方文档：DeepSeek API 文档（`stream:true` + SSE + `data:[DONE]`、`stream_options.include_usage`、`thinking: {type, reasoning_effort}` 参数形状、`reasoning_content` delta、Anthropic 兼容端点 `https://api.deepseek.com/anthropic`）。
- 双端高亮对比是刻意实验（ADR-0011），验收 = 桌面/移动各跑一条带多代码块的长回复，记录流畅度与高亮观感；结论决定后续统一引擎方向。
- 插件配置是 ai-chatbox 首个 `contributes.configuration` 段，机制沿用 SDK 约定（storage key `config`，宿主配置页 schema 渲染）。
- 移动端 Shiki bundle 与 WASM 加载成本已知，属接受代价；若对比实验后 Shiki 胜出，宿主 API 封装将消除该成本。
