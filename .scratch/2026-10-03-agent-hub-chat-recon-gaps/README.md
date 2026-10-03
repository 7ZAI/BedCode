# Agent Hub 聊天还原缺陷登记（解析层 + 展示层）

**审计日期：** 2026-10-03
**审计范围：** agent-hub 日志页「会话详情 → 聊天记录」的 JSONL → 对话还原链路
**对比基线：** zcode / qoder 等成熟桌面端 GUI agent 的会话回放展示
**登记人/来源：** 会话审计（`wasm-apps/agent-hub/rust/src/usage_parse/` 四适配器源码逐行核对 + 本机真实会话文件实证：claude 1380 行含 597 assistant / 353 tool_use / 115 attachment；pi 159 条消息含 46 thinking / 72 toolCall）

> 目的：把审计确认的**解析层信息丢失**与**展示层体验差距**逐条登记，附实机证据、修复建议、优先级与 triage 状态，作为后续排期修复的单一真源。对外摘要见 `CHANGELOG_zh.md` / `CHANGELOG.md`（2026-10-03 条目）。

---

## A. 解析层缺陷（JSONL / SQLite 中有信息，未解析到或解析错）

> 代码位置：`bedcode-desktop/wasm-apps/agent-hub/rust/src/usage_parse/{claude.rs, pi.rs, codex.rs, opencode.rs}`；归一模型 `types.rs`（`NormalizedEvent` 仅 user/assistant/tool/system 四角色 + text/model/tokens，**无结构化承载 thinking / toolUse / toolResult 的属性面**——多数展示层修复依赖此扩展）。

### A1. pi 适配器 `toolCall` 块整体丢失（真 bug）
- **Status:** `done（2026-10-03 已修复）`
- **优先级:** P0（信息丢失级）
- **现象：** 助手消息复用 claude 的 `assistant_display_text`，只匹配 claude 拼写 `tool_use`；pi 实际内容块类型是 **`toolCall`**（`{type, id, name, arguments}`）→ 未命中任何分支被静默丢弃，助手消息里永远看不到工具调用本身，只见后续 toolResult
- **实机证据：** pi 单会话 72 个 `toolCall` 块全丢（`.pi/agent/sessions/--home-binblink-project-tauriProject-BedCode--/2026-09-04T17-54-25-*`）
- **修复建议：** `assistant_display_text`（claude.rs，pi.rs 复用）的 `tool_use` 分支把匹配令牌扩展为 `"tool_use" | "toolCall"`；`arguments` 是**对象**（pi）或字符串（claude/codex 原文），展示为紧凑 JSON 摘要（预计 `args_summary` ≤120 字符）
- **修复实现：** claude.rs `assistant_display_text` 同时命中 `tool_use`（取 `input`）与 `toolCall`（取 `arguments`），经 `common::tool_args_summary` 输出紧凑 JSON 摘要，截断护栏 `TOOL_ARGS_CAP=120`；空对象不显示 `{}`
- **验收方向：** pi 夹具单测覆盖 toolCall 块 → 事件文本含工具名与参数摘要；不回归 claude/opencode/codex 既有断言 ✅（夹具：`claude_shared_display_handles_pi_tool_call_blocks` + `pi_tool_call_blocks_shown_with_args`）

### A2. `thinking`（推理）块两种适配器全部丢弃
- **Status:** `needs-triage`（是否需要单独的可折叠推理块展示，等待 UI 决策）
- **优先级:** P0/P1（信息丢失级；展示形态需设计）
- **现象：** claude 与 pi 的助手内容块 `thinking` 在 `assistant_display_text` 中为 `=> {}` 空分支；用户看不到模型推理过程
- **实机证据：** pi 单会话 46 个 thinking 块（含 `thinkingSignature`）；claude 会话本轮无 thinking 块但 API 流量存在
- **修复建议：** 归一模型扩展结构化字段承载推理（如 `reasoning` 分区），展示为**独立可折叠块**（默认收起，与本次 B1 折叠模式同构）；claude `thinking` 的 `signature` 无需展示
- **验收方向：** 各适配器夹具含 thinking 块 → 事件含推理内容（不丢）；展示层默认折叠可展开；en/zh i18n

### A3. claude `tool_use` 只显示工具名，参数（input）不显示
- **Status:** `done（2026-10-03 已修复，随 A1 同套路）`
- **优先级:** P1（信息缺失级）
- **现象：** `assistant_display_text` 对 `tool_use` 只输出 `tool_use · {name}`；代码注释声称「名称 + 参数/结果摘要」但实现没有参数。用户看不到 bash 命令 / Read 路径等关键调用内容
- **实机证据：** claude 单会话 353 个 tool_use 块全部只有名字
- **修复建议：** 与 A1 同套路（input 紧凑摘要 + 截断护栏 ≤120–200 字符）
- **修复实现：** `tool_use` 分支取 `input`（对象 → 紧凑 JSON）经 `tool_args_summary` 摘要，超长截断到 120 字符 + 省略号
- **验收方向：** 夹具断言事件文本含 `input` 关键字段；超长参数被截断不撑爆序列化 ✅（夹具：`claude_tool_use_shows_args_summary` + `claude_tool_use_truncates_oversized_args`）

### A4. claude `tool_result` 信息不完整：`is_error` 不标记 / `tool_use_id` 不对接 / 图片块静默丢弃
- **Status:** `done（2026-10-03 已修复：error 字段 + 配对键承载 + 非 text 占位；卡片渲染仍归 B3）`
- **优先级:** P1
- **现象：** ① `is_error: true` 的结果与其他结果同色同形，错误无视觉标记；② `tool_use_id` 未与对应 `tool_use` 配对（成熟 GUI 把调用+结果渲染成一张卡片）；③ tool_result 内容数组里的非 text 块（图片/二进制）被 `extract_text` 静默过滤
- **修复建议：** 归一模型给工具事件加 `error: bool`（展示红色左条/前缀）；按 `tool_use_id` 配对调用与结果（先后序栈）；非 text 块给「[图片/二进制块，原始 JSONL 可查]」占位而非消失
- **修复实现：** ① `NormalizedEvent` 新增 `error: bool`（claude `is_error` / pi `isError` 落地，wire 形状 `error`）；② 新增 `tool_use_id: Option<String>`（claude `tool_use_id` / pi `toolCallId` / codex `call_id` 承载，wire 形状 `toolUseId`）；claude 侧用**先后序栈**把 tool_result 配对到同 id 的 tool_use，事件文本 `tool_result · {工具名} · …`；③ `common::tool_result_text` 对非 text 块输出 `[{type} 块，原始 JSONL 可查]` 占位（claude tool_result 与 pi toolResult 共用）
- **验收方向：** 夹具含 is_error=true → 事件带错误标记；配对正确；图片块不消失 ✅（夹具：`claude_tool_result_marks_error_keeps_id_and_placeholder` + `pi_tool_result_error_flagged_and_image_placeholder` + `event_wire_shape`）

### A5. claude `attachment` 行不解析为附件条目
- **Status:** `done（2026-10-03 已修复）`
- **优先级:** P1（信息完整度）
- **现象：** `type=attachment` 行（读过的文件、贴图、长上下文注入）完全跳过——用户看不到「这个会话看过哪些文件」
- **实机证据：** claude 单会话 115 条 attachment
- **修复建议：** `attachment` 行解析为 system 角色附件条目（文件名/类型/来源），可折叠展示；不进标题与 token 聚合
- **修复实现：** claude.rs 新增 `"attachment"` 分支：取 `attachment.type`（+ `filename` / `prompt` / `newDate` 短摘要 ≤120）归一为 system 事件；天然不进标题与 token 聚合（只读聚合计数）
- **验收方向：** 夹具断言 attachment 产生事件；统计口径不受污染（不计数 session）✅（夹具：`claude_attachment_lines_become_system_entries`）

### A6. claude `ai-title` 行未用于会话标题
- **Status:** `needs-triage`
- **优先级:** P2（体验优化）
- **现象：** 标题只取首个非 meta user 消息截断 120 字符；官方 `type=ai-title` 行自带模型生成的简洁标题（实机单会话 56 条），内容更佳却未用
- **修复建议：** 有 `ai-title` 时优先作标题，回退现状
- **验收方向：** 夹具含 ai-title → 标题取之；无 ai-title 行为不变

### A7. claude user 正文不截断（opencode 侧已截断的对称缺口）
- **Status:** `done（2026-10-03 已修复）`
- **优先级:** P1（健壮性/风险）
- **现象：** opencode 侧对超大 payload（part.data 达 150KB）有 SQL/展示双层截断护栏（600/400），**claude user 消息正文无任何截断**直入事件流 → 单条超长 user 消息可能撑爆 WATM 边界序列化（与 opencode 注释警示同因）
- **修复建议：** user 文本入事件前 `truncate_text(text, 2000)`（与 assistant 同口径），标题仍取截断前原文前 120
- **修复实现：** claude 与 pi 的 user 分支事件文本统一 `truncate_text(&text, 2000)`（pi 对称补齐）；标题仍从原文截取 120
- **验收方向：** 超长 user 夹具 → 事件文本 ≤2000 字符 + 省略号；标题不受影响 ✅（夹具：`claude_long_user_text_truncated_title_kept` + `pi_long_user_text_truncated`）

### A8. opencode 事件：reasoning / tool 输出被压平进正文，无结构化区分
- **Status:** `needs-triage`
- **优先级:** P1（结构基础，B3 的前置）
- **现象：** `parse_opencode_events` 把同 message 的 text + `reasoning · …` + `tool · name (status) · …` 拼接成一条杂烩正文（`body + extra.join`），展示层无法把推理/工具调用渲染为独立块
- **修复建议：** 归一模型扩展结构化属性面（同 A2），opencode part 按类型归位；纯结构 part（step-*/patch/compaction）维持不进流
- **验收方向：** 夹具多 part 消息 → 结构归位正确；既有「message 级去重」断言不回归

### A9. codex：两处已知限制（**非缺陷，登记防误修**）
- **Status:** `wontfix（登记）`
- ① 推理展示：官方密文不产出事件（渲染是乱码），只取 `summary`——合理策略，不修
- ② 助手消息无 token 明细：`token_count` 是轮级事件无法归到具体消息——结构性限制，不修（会话级聚合已正确）

### A10. pi 每轮都写一条空助手消息（`content: []` + 空 usage）→ 一串零 token 气泡
- **Status:** `needs-triage`（展示层已兜住；解析层是否丢事件待定）
- **优先级:** P1（噪声源，用户实机报障「pi 里经常出现 ↑0 ↓0 ⚡0 +0」）
- **现象：** pi 每轮对话至少写一条 `content: []` 的 assistant 消息（占位用），且 `usage` 是**空对象**。guest 的 `has_usage` 判据是「usage 是对象」，于是这条事件带上一份**五项全 0** 的 `TokenUsage` 落到事件流；展示层就渲染出一个只有「助手 / 模型 / 时间 / ↑0 ↓0 ⚡0 +0」的空气泡
- **实机证据：** `~/.pi/agent/sessions/--home-binblink--/2026-09-20T12-38-02-768Z_*.jsonl` 单会话 83 条 assistant 消息中 **58 条是 `content: []` + 零 usage**（70%），另 7 条是 `thinking`/`toolCall` 无正文
- **修复建议：** 解析层在 push 前跳过「展示正文为空 **且** 五项 token 全为 0」的助手事件（信息量为零，且能省下 WATM 边界上的序列化字节）；**展示层已先兜住**（2026-10-03，见 B3），两者不冲突：解析层丢事件后展示层的判据自然恒真
- **验收方向：** 夹具含 `content: []` + `usage: {}` → 不产事件（或展示层不渲染行）；有真实 usage 的空正文助手事件仍保留（推理-only 行将来要显示思考内容）

---

## B. 展示层差距（对比 zcode / qoder 等成熟 GUI）

> 代码位置：`bedcode-desktop/wasm-apps/agent-hub/src/components/SessionLogsTab.vue`（聊天视图）+ `src/styles.css`（`.ah-msg-*`）+ `src/utils/format.ts`。

### B1. 模型输出无折叠，长回复直接铺满视口 —— ✅ 已修复（2026-10-03）
- **Status:** `done（2026-10-03）`
- **实现：** 助手正文 > `COLLAPSE_THRESHOLD_CHARS`（500，`src/utils/format.ts`）默认**折叠**为 3 行 clamp 预览 + 「展开全文 / 收起」按钮（`aria-expanded` / `aria-controls`）；短消息不折叠无按钮；仅 assistant 角色；切换会话重置折叠态。设计依据 ui-ux-pro-max「Truncate with ellipsis and expand option」
- **验证：** agent-hub 376 用例（新增 5 条）/ 桌面全量 vitest 1432 全绿；eslint 0 error；vue-tsc 无本次新增 error

### B2. 纯文本渲染，无 markdown / 代码高亮 —— ✅ 已修复（2026-10-03，自带零依赖子集渲染器）
- **Status:** `done（2026-10-03）`
- **实现：** 新增 `src/utils/markdown.ts`（纯函数、零依赖、零 DOM）：**先转义后拼标签**，输出只含白名单标签，无需 sanitizer；链接**不产生 href**（宿主已撤 shell/opener + CSP `connect-src 'none'`，可点链接只会变成死链），URL 挂 `title`；图片只留 alt（零资源访问红线）。子集：段落（软换行 → `br`）/ ATX 标题 / 有序无序列表（含两级缩进嵌套）/ 引用 / 分隔线 / 围栏代码块（``` 与 ~~~）/ GFM 管道表格 / 行内码·粗体·斜体·删除线·链接
- **只渲染助手正文**：用户行是「我说的话」、工具行是命令原文，都不该排版；样式作用域在 `.ah-md` 下走元素选择器（v-html 内容无模板引用，也避开 S5 死规则守门）
- **折叠态不生成 HTML**：`markdownPlainPreview` 剥标记给纯文本，配合 CSS line-clamp（line-clamp 只对纯文本行数可靠）；展开态才产出结构化 HTML —— 顺带省掉大多数长消息的一次解析
- **为什么不用 marked**（原 needs-triage 的裁决）：agent-hub 是**只读日志查看器**（无流式、无输入框），为一个读记录的视图引解析器 + sanitizer 两个运行时依赖不划算；双端 ai-chatbox 已有各自 marked + DOMPurify，但那两处都在流式对话主链路上
- **验证：** `src/__tests__/markdown.test.ts` 25 条（14 条行为契约 C-M01–C-M14，含 XSS 反例 / javascript: URL / 属性注入 / 裸数字占位符回归 / CRLF）+ 组件层 3 条；agent-hub 423 用例全绿

### B3. 工具调用无独立可折叠卡片；失败结果无视觉标记 —— 🟡 部分已修复（2026-10-03）
- **Status:** `done（展示层可做部分）` + 残余依赖解析层
- **已实现（展示层）：** 工具行按 guest 的 ` · ` 约定切成**卡头（类型 · 名称）+ 卡身（参数 / 输出）**——四适配器四种形态全覆盖（claude `tool_use`/`tool_result`、codex/opencode `tool · 名称 (状态) · 卡身`、pi `名称 (error) · 卡身`），未知形态整条落卡身不编头；卡身超 400 字**默认收起**（同 B1 模式，按钮文案区分「展开详情」），阈值以下直接铺开；`tool_use · Bash` 无卡身时不渲染空正文块；pi / opencode 的尾部 `(error)` → 危险色左条 + 卡头转语义危险色
- **工具输出上限 400 → 1000（用户要求，2026-10-03）**：guest 四个适配器（codex / opencode 常量 + claude / pi 字面量）统一提到 1000 字符——400 字符点开也读不完命令输出；展示层镜像 `GUEST_TEXT_CAPS.toolOutput` 同步，折叠阈值随之从 200 提到 400（上限的 40%）。**展示层不做字符级截断**：DOM 里始终是 guest 给的全文，阈值只决定是否给展开控件
- **空泡与零 token 处置（用户实机报障带出）：** 正文空且五项 token 全为 0 的助手行整条不渲染（pi 逐轮空消息，A10）；全零 token 行不再出——「↑0 ↓0 ⚡0 +0」不是信息，只会让人以为统计坏了。正文空但 token 有量的行仍保留（将来 A2 的推理内容要有地方落）
- **仍需解析层（未做）：** ① **调用↔结果配对**（需 `toolUseId`，前端类型 `NormalizedEventView.toolUseId` 已由在途任务加入，guest 未下发前不做）；② **claude / codex 的 `is_error`**（需 guest 带出，前端有 `error` 字段在途）；③ **参数（input/arguments）摘要**（A3）。本次的文本 `(error)` 启发式是**兜底**：`error` 字段落地后改读字段，启发式退为兼容路径
- **验证：** `format.test.ts` 新增 14 条（C-T01–C-T07 / C-X01–C-X04 / 阈值关系）+ 组件层 5 条（卡头卡身切片 / 无卡身不渲染空块 / 失败与非失败 / 默认收起与互切 / 短输出不折叠）

### B4. 正文解析层截断无「查看完整原文」通道 —— 🟡 展示层部分已修复（2026-10-03）
- **Status:** `done（展示层可做部分）`；guest 侧「按行取原文」命令仍未做
- **已实现：** guest 的 `truncate_text` 只在真截断时补省略号收尾，展示层据此识别**疑似截断**（`format.ts::looksTruncated`：**末字符是省略号** + **最后一个 ` · ` 段长于最小上限 120**）。判定落在**末段**而非整条长度——工具卡内可能有嵌套上限（claude 新增的 `TOOL_ARGS_CAP=120` 参数摘要让 `tool_use · 名称 · 参数…` 整条才 ~140 字，按工具 400 判整条会漏报）。排除阀取**最小**上限（120）而非各行自己的上限，是为了让真截断不漏、只承担「自然长句恰好以省略号收尾」的误报风险（提示语是建议式的）。上限矩阵镜像在 `GUEST_TEXT_CAPS`（消息 2000 / 工具输出 **1000** / 参数摘要 120 / 标题 120），漂移即可见。在事件卡里给一行提示 + 「查看原始 JSONL」按钮直达 raw 页签 —— 对应 ui-ux-pro-max ux-guidelines「Essential Text Truncation（Critical）：必须给可见的完整详情路径」
- **仍未做：** 按需整段读取原文需 guest 新命令（「按行取原文」），属解析层扩展；raw 页签仍是兜底通道
- **验证：** 组件层 2 条（达上限提示 + 跳 raw 页签 / 等长未截断不报）

---

## 修复顺序建议

1. **第一批（P0/P1 信息丢失级，独立小改，各配夹具即收）—— ✅ 已全部完成（2026-10-03，解析层）**：A1（pi toolCall）→ A7（user 截断护栏）→ A3（tool 参数摘要）→ A4（is_error 字段 + tool_use_id 配对键 + 非 text 占位 + claude 配对文本）→ A5（attachment 条目）。wire 新增 `error` / `toolUseId`（向后兼容），agent-hub Rust 162 用例 + 前端 423 用例全绿，eslint 0 error
2. **第二批（结构基础，需归一模型扩展 `types.rs` 与 WIT 契约评估）**：A2 + A8（thinking / tool 结构化）→ B3 残余。**B3 残余的解析层前置已就位**：`error` / `toolUseId` 已由 guest 下发、tool_use 参数摘要已做、claude tool_result 文本已带配对工具名——展示层只需改读字段（文本 `(error)` 启发式退为兼容路径）、配对渲染可读 `toolUseId` 或直接用文本配对结果
3. **第三批（体验，需要评估）**：A6（ai-title）→ B4 残余（guest「按行取原文」命令）→ B2 可选增强（代码语法高亮）

> 2026-10-03 已完成：**B2 全量** + **B3 展示层** + **B4 展示层**（本次交付，见 CHANGELOG）。B2 自带的 markdown 渲染器**故意不带语法高亮**（hljs/shiki 是重量级依赖，日志查看器不值得）；若后续要加，走 ai-chatbox 同款（marked + highlight.js / shiki）即可，且本仓已有先例。

> 注意（§5.1 归属裁决）：归一模型扩展仅是插件事务（guest 内部结构 + 插件前端），不触碰宿主；若「按行取原文」需要新宿主能力，必须先过 B1–B6 三问裁决并读 ADR 0022，禁止新业务语义进宿主。

## 相关代码位置速查

| 域 | 文件 |
| --- | --- |
| claude 适配器 | `wasm-apps/agent-hub/rust/src/usage_parse/claude.rs`（`assistant_display_text` / `tool_result` 分支） |
| pi 适配器 | `wasm-apps/agent-hub/rust/src/usage_parse/pi.rs`（复用 claude 的 `assistant_display_text`） |
| codex 适配器 | `wasm-apps/agent-hub/rust/src/usage_parse/codex.rs`（`codex_response_item_event`） |
| opencode 适配器 | `wasm-apps/agent-hub/rust/src/usage_parse/opencode.rs`（`parse_opencode_events`） |
| 归一模型 | `wasm-apps/agent-hub/rust/src/usage_parse/types.rs`（`NormalizedEvent`） |
| 聊天视图 | `wasm-apps/agent-hub/src/components/SessionLogsTab.vue`（`.ah-chat` 区块） |
| 消息样式 | `wasm-apps/agent-hub/src/styles.css`（`.ah-msg-*`，2600 行起） |
| 折叠阈值 | `wasm-apps/agent-hub/src/utils/format.ts`（`COLLAPSE_THRESHOLD_CHARS` / `COLLAPSE_THRESHOLD_TOOL_CHARS`） |
| 工具卡切片 / 截断识别 | `wasm-apps/agent-hub/src/utils/format.ts`（`splitToolText` / `looksTruncated` / `truncateCapFor`） |
| Markdown 渲染 | `wasm-apps/agent-hub/src/utils/markdown.ts`（`renderMarkdown` / `markdownPlainPreview` / `escapeHtml`） |