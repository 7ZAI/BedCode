# AI Chatbox 重构 — 进度跟踪（Step 1-4 完成）

> 与 `C:\Users\binblink\AppData\Local\Temp\bedcode-ai-chatbox-handoff.md` 同步。完整规格见 `spec.md`（同目录）。

## 总进度

| 步骤 | 状态 | 说明 |
|------|------|------|
| Step 1 宿主批量授权接口 | ✅ | 双端 SDK+宿主 4 处 cargo check + 242/95/5/2 tests |
| Step 1 移动端 FsAuthDialog | ✅ | 新建并挂载 App.vue |
| Step 2 SSE usage 透传 | ✅ | 双端 done 事件携带 usage |
| Step 3 桌面插件重构 | ✅ | Rust 全量重写 + 前端全量重写 + i18n + 测试 + 构建通过 |
| **Step 4 桌面 UI 视觉验证** | ✅ | 本会话完成：dev-shell mock（插件工程内）+ 6 轮 vision 审查迭代 |
| Step 5 移动插件重构 | ✅ | 14 cargo test + 14 vitest + 构建通过 |
| Step 6 移动 UI 视觉验证 | ✅ | 输入区重构为 DeepSeek/Claude 式后 vision 通过 |
| Step 7 文档 | ✅ | CONTEXT.md 术语段 + ADR 0006/0007 |
| Step 8 端到端验证 | ✅ | 授权四路径 + 落盘 + 恢复 + 重命名/删除 + 撤销授权（本会话完成） |

## Step 4 本会话改动（均未 commit）

### 插件 UI（bedcode-desktop/plugins/ai-chatbox/src/）
- **ChatMessage.vue**：
  - 硬编码头像 `'我'` → user 用 SVG 人形图标（i18n 安全）
  - 新增 hljs 语法高亮配色（浅/深两套，低饱和暖色系与宿主协调；此前只调 `hljs.highlightElement` 无主题 CSS → 代码块无颜色）
  - token 用量改宿主 tag 风格（`rounded-tag bg-[var(--bg-hover)]`）
  - **消息左右分列**：user 右对齐（`flex-row-reverse` + `text-right`），assistant 左对齐
- **ChatView.vue**：
  - 空态 emoji（🤖/💬）→ SVG 图标（bot / message-square）
  - 空态加辅助文案 `emptyHint`（支持 DeepSeek/Qwen/OpenAI/Anthropic）
  - header 标题语义：无对话时显示 `title`（AI 对话）而非"新对话"
  - systemPrompt 按钮图标：齿轮 → 文档图标（语义区分）
  - **模型 Select 从 header 移到输入区**（输入框上方一行"模型 [Select]"），header 只留供应商 Select
  - header 标题/regenerate 按钮加 tooltip
- **ConversationList.vue**：空态加 SVG 图标 + 引导文案 `noConversationsHint`，垂直居中（列表容器改 flex flex-col）
- **ProviderConfigPage.vue**：预设项选中态高亮；返回按钮加 hover 背景

### 宿主 i18n（bedcode-desktop/src/locales/{zh-CN,en}/desktop.ts）
- 修复缺失 key：`delete`（ConversationList 删除按钮 tooltip）、`name`（ProviderForm 名称 label）——此前渲染为裸 key 路径
- 新增 key：`title`（AI 对话面板名）、`emptyHint`、`noConversationsHint`、`model`
- zh/en 同步

### dev mock（插件工程内，不碰 dev-shell）
- 新建 `bedcode-desktop/plugins/ai-chatbox/src/dev-mock.ts`：
  - `import.meta.env.DEV` 时由 index.ts 的 activate() 注册（生产构建自动排除）
  - 补齐宿主 `desktop.plugin.aiChatbox.*` 文案（dev-shell 无宿主 locale；`context.i18n.getI18n().global.mergeLocaleMessage` 注入）
  - 8 个命令 mock（list/get/save/delete/chat-stream/chat-complete/fetch-models）
  - **默认空态；URL `?mock=1` 才预置供应商 + 对话数据**
  - 流式模拟：30ms/chunk 6-12 字符（约 2s 完成，太快/太慢都会误导截图评审）
- index.ts：activate() 中 `if (import.meta.env.DEV) await registerDevMock(context)`；deactivate() 调 disposeDevMock()

### dev-shell 使用方式（重要）
- **不改 dev-shell 任何代码**。`cd bedcode-desktop/plugins/ai-chatbox && npx bedcode-plugin-desktop dev` 启动（BEDCODE_DEV_PLUGINS 注入）
- 截图：`.scratch/cdp-ai-chatbox.mjs`（单页）+ `.scratch/cdp-ai-chatbox-flow.mjs`（流程：打开面板→输入发送→流式中/完成/配置页）
- Chrome headless 直连（`--remote-debugging-port` + CDP），vision subagent 审查

## 踩坑记录

1. **dev-mock.ts 长字符串里嵌套 JS 空串 `''` 会提前闭合外层单引号**（TS1005）——内部代码示例的空字符串改 `""`
2. **hljs 调了 highlightElement 但没 import 主题 CSS → 代码块有 class 无颜色**——在组件 scoped style 用 `:deep(.hljs-*)` + 宿主 token 定义两套配色（不 import 官方主题，避免与宿主 token 冲突）
3. **mock 流速度影响评审**：70ms/3-7字符 × 400 字符 ≈ 6s，截图时仍流式中（被误判为"光标残留/停止按钮不还原"）——30ms/6-12 字符 ≈ 2s
4. **npm workspace 依赖提升**：插件工程 node_modules 为空，依赖实际在 `bedcode-desktop/node_modules`（workspaces）——`npx bedcode-plugin-desktop` 从根可用
5. **vision 通用审美建议需对照宿主风格过滤**：气泡背景/选中态竖条/Provider›Model 复合控件等建议与宿主扁平风格（bg-hover 选中、无卡片）相悖，遵循宿主不采纳
6. **停 dev server 禁 taskkill //F //IM node.exe**（杀所有 node 进程）——用端口定位 PID 精确 kill

## 验证状态
- 插件：`vue-tsc --noEmit` 干净 + vitest 17 passed
- 宿主：`npm run test:run` 241 passed（含 locale 校验）
- vision 6 轮审查：空态 2 轮 + 实态/流式/配置页 4 轮，已通过（无遗留严重问题）

## Step 5 起点勘察结论
- 移动插件 rust：`ai_client.rs`/`commands.rs`/`db.rs`/`lib.rs` 旧结构 → 需新建 `store.rs`/`client.rs`（镜像桌面）
- 移动插件 src：components/composables/index.ts/types.ts 旧前端 → 需全量重写
- 移动 SDK 有 dev-shell：`bedcode-mobile/packages/plugin-sdk-mobile/dev-shell`
- 数据目录：`config_get(AppDownloadsDir)` + `/ai-chatbox/`；pluginType wasm（navtab+toolbox，无 sidebar）；Rust 无 WIT 走 func_wrap
- **移动宿主 locale**：`bedcode-mobile/src/locales/{zh-CN,en}/mobile.ts` 的 aiChatbox 段需同步（Step 3 只改了桌面 desktop.ts）

## Step 5（移动插件重构）— 2026/8/11 会话 4 完成

### 宿主
- `bedcode-mobile/src/locales/{zh-CN,en}/mobile.ts`：plugin 段新增 `aiChatbox` 段（57 个 key，与桌面 desktop.ts 同步，zh/en 双语言）

### 移动 SDK（wasm_entry! 宏 host 目标兼容）
- `packages/plugin-sdk-mobile/rust/src/wasm.rs`：wasm_entry! 宏整体包 `#[cfg(target_arch = "wasm32")] mod __bedcode_wasm_exports`——host 目标（cargo test）不生成 no_mangle 导出，避免对 host_* import 的未定义符号链接失败；wasm 构建不受影响（已验证 ai-chatbox / auto-task / file-transfer 三个插件 wasm 构建均通过）

### 移动插件 rust（全量重写）
- 删 `ai_client.rs` / `db.rs`；新建 `store.rs`（JSONL 持久化，泛型 `H: HostFs + HostLog`，无 tracing 走 host.log_warn）+ `client.rs`（镜像桌面，请求构造纯函数）
- 重写 `commands.rs`（8 命令，CommandArgs）+ `lib.rs`（activate：config_get(AppDownloadsDir) + fs_request_auth 集中授权 → 拒绝返回 Err 提示重新启用；DATA_DIR OnceLock）
- Cargo.toml version 2.0.0；测试：**cargo test 13 passed**（host 目标，MockHost 实现 HostFs+HostLog）；wasm release 构建通过

### 移动插件前端（全量重写）
- 删 PromptOptimizeDialog / ProviderSidebar / usePromptOptimizer
- types.ts / useAiConfig.ts / useAiChat.ts（镜像桌面，错误分类 key 前缀 `mobile.plugin.aiChatbox.*`）
- 组件 7 个：ChatView（移动布局：自绘 header + 消息流 + 底部输入区 + **对话列表底部抽屉 Teleport z-50** + 配置页全屏覆盖）、ChatMessage（移动 token + hljs 深/浅两套配色 `:global(html:not(.dark))` 覆写 + 常显操作条）、ChatInput（**DeepSeek/Claude 式输入区**：textarea 在上 + 底行左下模型 pill Select sm + 右下圆形发送/停止，空输入灰底禁用）、ConversationList（抽屉内用，操作常显）、ProviderConfigPage（单栏：预设 chips 横滑 + 自定义列表 + 表单）、ProviderForm / ModelListEditor（44px 触摸目标 + --mobile-* token）
- index.ts（toolbox + navtab 注册，标题随语言重注册）+ i18n（navTitle/toolboxTitle）+ dev-mock.ts（注入 `mobile.plugin.aiChatbox.*` 宿主 key + 8 命令 mock + ?mock=1 预置）+ vite-env.d.ts
- package.json：加 marked/highlight.js/vitest，version 2.0.0；测试：**vue-tsc 干净 + vitest 14 passed**；`bedcode-plugin build` 通过

## Step 6（移动 UI 视觉验证）— 进行中（输入区重构后待最终确认）

- dev-shell 启动：`cd bedcode-mobile/plugins/ai-chatbox && npx bedcode-plugin dev`（**默认端口 5173**！勿加 --port）
- 截图：`.scratch/cdp-mobile-partA.mjs`（聊天流程）/ `cdp-mobile-partB.mjs`（配置页），puppeteer 连真实 Chrome 9222（复用用户可见浏览器）；输出 `.scratch/step6/`
- vision 审查 3 轮：抽屉/消息/配置页通过；**输入区按用户要求重构为 DeepSeek/Claude 式**（pill 左下 + 圆形发送右下）后通过
- 遗留：vision 🟡 建议（禁用态对比弱、流式停止态）为品味级，不阻塞

## 踩坑（本会话新增）

1. **`bedcode-plugin dev` 默认端口 5173**（cli.js `flags.port || 5173`）；手动 `--port 5199` 会被用户纠正为标准端口
2. **真实 Chrome（可见窗口）Transition 卡起始帧**：窗口/标签在后台时 rAF 暂停，Vue Transition 卡在 enter-from（opacity 0），screenshot 拿到"抽屉不可见"帧；修复：puppeteer `Browser.setWindowBounds normal` + `page.bringToFront()`；headless Chrome 无此问题
3. **裸 CDP ws 连真实 Chrome 151 时 Page.captureScreenshot 挂起**（Runtime.evaluate 正常）——改用 puppeteer-core（browser-tools 依赖）
4. **puppeteer 长连接跑 >6 步后 evaluate/截图挂起**——拆两段脚本（partA 聊天流程 / partB 配置页）规避
5. **截图路径双重拼接**：脚本 OUT_DIR 用相对路径且 cwd 在 .scratch 内 → 存到 .scratch/.scratch/step6；统一绝对路径
6. **截图脚本选择器误命中**：`find(e => e.textContent.includes(...) && children.length<=2)` 命中 sheet-panel 容器（无 @click）→ 点击无效；改用 `textContent.trim() === 标题` 精确匹配叶节点
7. **vision 对"遮罩缺失"的误判**：rgba(0,0,0,0.6) 遮罩在 #0a0a0f 深色页面上对比弱，截图看"像没有遮罩"——实际存在，属深色主题固有观感

## Step 7（文档）— 完成

- `CONTEXT.md` 新增"AI 对话 (AI Chatbox)"术语段；`docs/adr/0006-jsonl-conversation-store.md` + `0007-activation-gated-directory-auth.md`

## Step 8（端到端验证）— 2026/8/11 会话 5 完成

### 验证结果（桌面 tauri:dev 真实宿主）

1. **授权四路径**：Loaded→启用→首次弹窗 ✅ / 拒绝→Activation failed（Error 状态+路径+重新启用提示）✅ / 同意→授权保存+store::init ✅ / 已授权短路无弹窗 ✅；30s 超时与拒绝同路径未单测
2. **落盘闭环** ✅：发送后 `conversations/{id}.jsonl`（meta 首行+用户消息行）+ index.jsonl 条目
3. **重启恢复** ✅：重启 app 后对话列表从 index.jsonl 恢复
4. **重命名/删除** ✅：列表项内联重命名（Enter 确认）→ conv 首行 meta + index 同步；删除 → 文件删除 + index 清空 + UI 空态
5. **撤销授权** ✅：sqlite 删 fs_granted_paths → 停用→启用→弹窗；拒绝→宿主日志 `denied by user → activate failed` + UI Activation failed + 启用偏好自动撤销；允许→授权恢复 + 激活成功
6. 换模型/停止/重生成/上下文超限：依赖流式（真实 API key），dev-shell 已验 UI，无真实 key 跳过

### Bug 5（宿主 fs_read，本会话新发现+修复+已验证）：文件不存在时返回 Err 违反 SDK 契约

- 现象：Bug 3 修复后落盘仍失败（conversations/ 从不创建、index.jsonl 0 字节）；UI 正常 + 401 提示（错误被吞）
- 排查：宿主日志只有 fs_auth allowed（无 mock 痕迹——**Bug 4 的 mock 劫持判断实际不成立**，打包产物 `import.meta.env.DEV=false` mock 本就不注册）；发送流程 4 次访问全在 conv 文件、无 index 访问 → save_conversation 在 fs_write 前就中断
- 根因：宿主 `host_impl/fs.rs::fs_read` 对 NotFound 返回 `Err("fs error: file read failed")`，而 SDK `HostFs` 契约明确"文件不存在返回 `Ok(None)`"；store.rs 的 `unwrap_or_default` / `let-else` 永远走不到 → 新建对话保存链全断
- 修复：双端宿主——桌面 `wasm_runtime/host_impl/fs.rs` NotFound → `Ok(None)`；移动 `wasm_runtime.rs::host_fs_read` NotFound → 返回 0 + out=(0,0)（wasm 侧映射 Ok(None)，与空文件编码一致）；其余 IO 错误仍 Err
- 验证：宿主测试桌面 249 / 移动 98 passed；app 重启后发送落盘成功；重命名/删除/恢复全链路通
- 附带：桌面插件 lib.rs 清 unused import `OnceLock`（Bug 1 遗留）

### Bug 4 定性修正

原判"tauri:dev 下 dev-mock 劫持真实宿主"不成立（打包产物 DEV 恒 false）；`isTauriHost()` 双保险保留（无害，dev-server 直出源码场景仍防劫持），Bug 3+5 才是落盘失败的完整原因链

## 踩坑（会话 5 新增）

8. **宿主日志的 fs_auth allowed ≠ 文件操作成功**：fs_read/write 结果不落日志（成功无日志、Err 只回 wasm 不记录），只能靠 fs_auth 调用次数/模式推断调用链
9. **`.plugin-toggle` 列表顺序会变**：`t[t.length-1]` 在插件列表顺序变化后误点其他行（误激活 file-transfer / 误停用 auto-task）；改用"行内文本定位 + 祖先找 toggle"精确选择
10. **puppeteer reload `waitUntil: 'networkidle2'` 超时**（WebView 长连接）：改用 `domcontentloaded` + 固定 sleep
11. **bash 模板字符串吞反斜杠**：evaluate 内正则 `[^\n]` 在 bash 双引号内被转义吃掉 → 正则报错；改用 String.fromCharCode(10) 分割行
