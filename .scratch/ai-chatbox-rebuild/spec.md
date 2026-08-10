# AI Chatbox 插件重构规格（桌面 + 移动）

Status: ready-for-agent

> 本规格由 grilling 会话全程决策汇编而成（含 `plan.md` 与代码勘察结论）。实现者无需再做重大决策；双端插件完全独立，所有改动需镜像执行。

## Problem Statement

现有 `ai-chatbox` 插件（桌面 `bedcode-desktop/plugins/ai-chatbox`、移动 `bedcode-mobile/plugins/ai-chatbox`，两个完全独立的插件）存在以下问题：

1. 对话历史存宿主 SQLite，用户不可见、不可备份，违背"对话日志落盘记录"的诉求
2. 代码里带 native/reqwest 路径，但实际构建只编 WASM（`--no-default-features --features wasm`）——死代码长期无人清理
3. 移动端 WASM 流式受宿主 SSE 解析限制，仅 OpenAI 格式可靠可用
4. 终端提示词优化功能（optimize-prompt / PromptOptimizeDialog / terminal 集成）与"纯 AI 对话"定位不符
5. 插件写文件无授权流程（宿主 fs_auth 弹窗机制从未被任何插件实战验证过，移动端前端甚至没有授权弹窗组件）

目标：重构为**纯 AI 对话插件**（类似 DeepSeek / Claude / ChatGPT），供应商可配置、对话日志 JSONL 文件落盘、激活期集中目录授权，同时成为宿主 fs_auth 弹窗授权机制的首次实战验证。

## Solution

- **纯 AI 对话**：多对话管理、流式输出、Markdown 渲染、模型切换、对话级 system prompt、token 用量显示
- **单一 OpenAI 兼容供应商协议**：DeepSeek / Qwen / OpenAI / Anthropic 四家内置预设 + 任意自定义供应商（只填 baseUrl + API key + 模型名），支持真实 `GET /models` 拉取模型列表
- **JSONL 文件作为对话历史唯一存储**（替代 SQLite）：数据目录位于插件目录之外（桌面 `{HomeDir}/.bedcode/ai-chatbox/`、移动 `{AppDownloadsDir}/ai-chatbox/`），卸载插件不清用户数据
- **激活期集中目录授权**：宿主新增批量授权接口 `fs_request_auth(paths)`（一次弹窗授权多个路径，前缀制持久化覆盖整个数据目录）；同意 → 激活成功；拒绝/超时 → 激活失败（Error 状态）+ 提示"目录授权被拒绝：{path}，请在插件设置中重新启用以再次授权"
- **宿主 SSE usage 透传**：`parse_and_emit_sse` 的 openai 分支将 usage 字段随 done 事件携带（双端都是宿主解析，双端受益）
- 移除：终端提示词优化、terminal 权限、native/reqwest 代码路径、SQLite 存储

## User Stories

1. 作为桌面用户，我希望插件启用时弹出一次目录授权请求，同意后插件正常激活，这样我的对话日志有明确的落盘位置且经过我的同意
2. 作为桌面用户，我希望拒绝授权后插件明确显示"目录授权被拒绝"及具体路径，这样我知道插件为什么没有启用
3. 作为桌面用户，我希望拒绝授权后重新启用插件能再次弹出授权请求，这样我改主意后无需卸载重装
4. 作为移动用户，我希望移动端同样有目录授权弹窗（当前移动端无此组件），这样授权体验与桌面一致
5. 作为用户，我希望授权范围是数据目录前缀（记住后不再重复询问），这样后续读写对话日志不打扰
6. 作为用户，我希望撤销授权后插件给出友好提示（"目录授权已失效，请在设置中重新授权"）而非裸错误，这样我知道如何恢复
7. 作为用户，我希望从预设目录选择 DeepSeek / Qwen / OpenAI / Anthropic 后只需粘贴 API key 即可开始对话（baseUrl/格式/模型自动填好），这样配置成本最低
8. 作为用户，我希望填写 key 后可一键"拉取模型列表"（真实调用 `GET /models`），这样模型名更新时无需手抄
9. 作为用户，我希望拉取模型失败时回退到预设模型列表且不阻塞使用，这样冷门模型或网络波动不影响配置
10. 作为用户，我希望支持自定义供应商（手填 baseUrl + key + 模型名），这样中转站/私有网关也能接入
11. 作为用户，我希望新建对话、切换对话、重命名对话、删除对话，这样对话可组织
12. 作为用户，我希望新对话标题默认为首条消息前 30 字，这样无需手动起名
13. 作为用户，我希望发送消息后模型回复流式逐字显示，这样等待体验与 ChatGPT 一致
14. 作为用户，我希望生成过程中可停止，这样答错方向时能及时打断
15. 作为用户，我希望支持"重新生成"（重跑最后一条用户消息），这样不满意可换一版答案
16. 作为用户，我希望消息以 Markdown 渲染、代码块高亮且可一键复制，这样阅读代码回复体验良好
17. 作为用户，我希望单条消息可复制文本、可删除，这样整理对话灵活
18. 作为用户，我希望对话内可随时切换模型，切换后新消息使用新模型，这样对比模型效果方便
19. 作为用户，我希望每条助手消息显示 token 用量（prompt/completion/total），这样对消耗有感知
20. 作为用户，我希望每个对话可设置独立的 system prompt，这样"聊天"与"写代码助手"等场景可分别定制
21. 作为用户，我希望长对话直接把全部历史发给模型，上下文超限时给出明确提示（建议新建对话），这样行为可预期
22. 作为用户，我希望对话历史以 JSONL 文件落盘且含完整元信息（模型、时间、system prompt、usage），这样可自行查看/备份
23. 作为用户，我希望重启应用后对话列表与消息完整恢复，这样历史不丢失
24. 作为用户，我希望删除对话同时删除其日志文件与索引，这样磁盘不留残留
25. 作为用户，我希望输入框支持多行（Enter 发送 / Shift+Enter 换行），这样贴代码方便
26. 作为移动用户，我希望移动端拥有与桌面一致的功能与数据模型（同一 Rust 核心编译 WASM + 同一前端组件），数据落在 Downloads/ai-chatbox/ 便于备份
27. 作为用户，我希望网络失败 / 非 200 / 无 key 等错误有明确的中文/英文提示，这样知道问题在哪
28. 作为用户，我希望界面语言随宿主切换（zh-CN / en 双语言同步），这样无需单独设置

## Implementation Decisions

### 宿主侧：批量目录授权接口（桌面 + 移动各自实现，不建共享 crate）

- **SDK**：`HostFs` trait 新增 `fn fs_request_auth(&self, paths: &[String]) -> Result<bool, HostError>`（返回是否全部同意；false = 拒绝或 30 秒超时）。桌面 SDK 同步更新 `wit/bedcode.wit` 的 `host-fs` interface（`request-auth: func(paths-json: string) -> result<bool, string>`，wit-bindgen 编译期生成绑定）；移动 SDK 无 WIT，走 `extern "C"` + wasmtime `func_wrap` 注册（`host_fs_request_auth`，返回 1 同意 / 0 拒绝 / -1 失败）
- **宿主实现**：`fs_auth` 新增 `check_batch(plugin_id, paths, op)`——逐路径跑 路径白名单 → 插件白名单 → 已授权前缀 短路放行，未授权路径合并为**一次**弹窗（payload：`{ requestId, pluginId, paths: [], path: <首项>, operation }`）；`respond(allowed, remember)` 同意时逐路径保存父目录前缀；`PendingRequest` 的 `path` 字段改为 `paths: Vec<String>`
- **前端弹窗**：桌面 `FsAuthDialog.vue` 支持 `paths` 数组展示（多路径列表、单路径保持原样）；移动端**新建** FsAuthDialog（监听 `plugin:fs-auth-request`，经 `plugin_fs_auth_respond` 回调宿主）并挂载 App.vue
- **不做**：不把 `com.bedcode.ai-chatbox` 加入任何白名单（走弹窗授权，作为 fs_auth 首次实战验证）

### 宿主侧：SSE usage 透传

- 桌面 `wasm_runtime/host_impl/http.rs` 与移动 `plugin/wasm_host.rs` 的 `parse_and_emit_sse`（openai 分支）：最后一个 data 块的 `usage` 字段随 done 事件携带（`{ done: true, usage?: {...} }`），向后兼容增量

### 插件侧（两个独立插件，各自全量重写）

- **plugin.json**：权限增 `fs:read`、`fs:write`，删 `terminal:input`/`terminal:output`；删 `terminal` contributes 与 `optimize-prompt` command；桌面保留 sidebar 视图，移动保留 navtab + toolbox
- **commands**：`chat-stream`（流式）、`chat-complete`（非流式，测试连接用）、`fetch-models`（GET /models）、`list-conversations`、`get-messages`、`save-conversation`、`save-message`、`delete-conversation`
- **Rust 结构**（`rust/src/`，单 wasm feature，无 reqwest/tokio）：
  - `store.rs`：JSONL 持久化。数据目录解析（桌面 `config_get(HomeDir)` + `/.bedcode/ai-chatbox/`；移动 `config_get(AppDownloadsDir)` + `/ai-chatbox/`）；`init()` 建缺省文件；`list_conversations()` 读 index.jsonl 按 updatedAt DESC；`get_messages()` 读对话文件跳过首行 meta；`save_message()` 读-拼-写整文件；`save_conversation()`/`delete_conversation()` 更新 meta + 重写索引；全部经 `host.fs_*`，错误带 `anyhow::Context`
  - `client.rs`：OpenAI 兼容请求构造（`/chat/completions`、Bearer、stream:true + `streamEvent` + `sseFormat:"openai"`）；`chat_complete`（stream:false）；`fetch_models`（GET /models 解析 `data[].id`）
  - `commands.rs` / `lib.rs`：activate 集中授权流程——`config_get` 计算数据目录 → `fs_request_auth([dataDir])` → 拒绝则返回 Err（含路径，提示重新启用重试）→ 同意则 `store::init()` → 激活成功
- **前端结构**（`src/` 全量重写）：`useAiChat`（会话/流式/停止/重生成/持久化，经 `context.commands.execute` + `context.events.on`）、`useAiConfig`（providers CRUD + 拉取模型）、组件 ChatView / ChatInput / ChatMessage（Markdown + 代码高亮 + 复制 + token 显示）/ ConversationList / ProviderConfigPage / ProviderForm（预设目录 + 自定义 + 拉取模型 + 测试连接）/ ModelListEditor；删除 PromptOptimizeDialog、ProviderSidebar；i18n zh-CN + en 同步
- **UI 实现与视觉验证工作流**（桌面/移动各自独立执行）：① `prototype` skill 在 dev-shell 中搭建页面骨架与视觉方向（对话列表 + 消息流 + 输入区 + 供应商配置页）→ ② 加载 `frontend-styles` skill 在插件工程 `src/` 中正式实现全部组件 → ③ dev-shell 运行（mock-context 提供命令 stub 与事件模拟）→ ④ Chrome headless 截图关键页面（空态/实态、流式渲染、配置页、授权拒绝态）→ ⑤ vision subagent 审查（桌面 `范围: 桌面应用内` / 移动 `范围: 手机内部`），对照对应宿主 UI 风格（CSS token 一致性）与项目规范（AGENTS.md / frontend-styles / i18n 双语言 / 禁原生控件外观），问题清单迭代修复直至通过。页面级重写必须走完整 ①→⑤，小修小改可 ②→⑤

### 数据文件格式（双端一致）

- `conversations/{convId}.jsonl`：首行 `{"type":"meta","id","title","createdAt","updatedAt","providerId","providerName","model","systemPrompt"}`，后续逐行 `{"type":"message","role","content","timestamp","model?","usage?":{"promptTokens","completionTokens","totalTokens"}}`
- `index.jsonl`：每行一个对话 meta（无消息），写入时按 updatedAt DESC
- `providers.json`：`{"providers":[{id,name,apiKey,baseUrl,apiFormat:"openai",models[],activeModel}],"activeProviderId","activeModel"}`（API key 明文，与现状一致）
- 写入策略：用户消息立即落盘；助手回复流结束后整体落盘（含 usage）；流中断也落盘已接收内容

### 行为约定

- 标题 = 首条消息前 30 字截断，可手动重命名
- 上下文全量发送；超限时提示"超出上下文长度，请新建对话"
- 旧 SQLite 数据**不迁移**（v1 无历史包袱，直接以空 JSONL 起步）
- 已知宿主行为：未授权前每次启动都会弹窗（宿主无"拒绝记忆"，30 秒超时自动拒绝）——本次作为 fs_auth 机制首次实战验证

## Testing Decisions

好的测试只测外部行为（给定输入 → 断言输出），不测实现细节；SSE 解析（宿主既有代码）不在测试范围，usage 透传由 E2E 顺带验证。

**接缝 1 — 宿主 `fs_auth::check_batch`（桌面 + 移动各自单测）**：mock PluginStorage + 无头上下文（无 AppHandle → 弹窗层保守拒绝）。断言：已授权前缀路径短路放行（true）；白名单路径放行（true）；未授权路径在无头上下文下返回 false；批量含已授权+未授权时只把未授权送入弹窗层。先例：宿主既有 `#[cfg(test)]`（如 auto-task hooks.rs、wasm_runtime http.rs tests）。

**接缝 2 — 插件 `store.rs`（Rust 单测）**：以内存 map 实现 `HostFs` trait 的 mock（读写删落到 map），测：新建对话文件（meta 首行）、追加消息读-拼-写、index.jsonl 按 updatedAt 排序重写、删除对话清理文件与索引、损坏 JSONL 行容错跳过。先例：同接缝 1。

**接缝 3 — 插件 `client.rs`（Rust 单测）**：请求构造纯函数（baseUrl 尾部斜杠处理、Bearer 头、messages 拼装、GET /models 载荷），不触网。

**接缝 4 — 前端 composables（vitest）**：mock PluginContext（dev-shell mock-context 为既有先例），测 useAiChat：发送→用户消息入列→命令调用参数（streamId/provider/messages）→事件 chunk 累积→done 保存助手消息（含 usage）→error 分支提示；重生成复用最后用户消息；useAiConfig：增删改、activeModel 同步、拉取模型落库。运行 `npm run test:run`（vitest run）。

**接缝 5 — 端到端手动验证**（fs_auth 首次实战，无法自动化）：授权四路径（同意 → 激活成功/拒绝 → Error 提示/30 秒超时 → Error/重新启用重试）；配 DeepSeek key 流式对话；落盘文件内容检查；重启恢复；删除对话；换模型；停止/重生成；上下文超限提示；撤销授权后写失败提示。

**接缝 6 — UI 视觉审查**（半人工，dev-shell + vision）：dev-shell 运行插件源码（mock-context 命令 stub + 事件模拟）→ Chrome headless 截图（对话空态/实态、流式渲染、供应商配置页、授权拒绝态）→ vision subagent 按对应宿主 UI 风格与项目规范审查 → 迭代直至通过。与接缝 5 同属无法自动化的验证环节，纳入端到端流程执行。

## Out of Scope

- 终端提示词优化（optimize-prompt、terminal 集成）——已确认移除
- Anthropic / Gemini / Ollama **原生** API 格式（保留 `apiFormat` 字段默认 `openai`，未来增量加格式）
- 上下文 token 裁剪（全量发送 + 超限提示）
- 消息编辑重发、历史搜索、费用估算、导出文件（数据本身即 JSONL，提供"打开数据目录"入口即可）
- 旧 SQLite 数据迁移
- API key 加密存储（系统钥匙串）
- 宿主 fs_auth 机制自身的重构或"拒绝记忆"策略（验证后如体验问题单独立项）

## Further Notes

- 双端插件完全独立（各自 `src/`、各自 Rust crate、各自 SDK），改动镜像执行；宿主改动 4 份（desktop/mobile × SDK/host）
- 桌面 SDK 有 WIT 契约（`wit/bedcode.wit` 单一事实来源，bindgen 编译期生成），移动 SDK 无 WIT 走 func_wrap —— 加 host 函数两端方式不同，见 Implementation Decisions
- 宿主 `fs_write` 为整文件覆盖（无 append），追加 = 读-拼-写；超长对话（>500 条）IO 放大可接受
- 宿主无目录扫描 API，对话列表依赖 `index.jsonl`
- 修改样式时加载 `frontend-styles` skill；i18n key 双语言同步；禁注释掉的代码；重要路径禁 `let _ =` 静默错误
