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
| Step 5 移动插件重构 | ⬜ | 移动端 rust 为旧结构（ai_client.rs/db.rs），需新建 store.rs/client.rs |
| Step 6 移动 UI 视觉验证 | ⬜ | 移动壳 dev-shell（`bedcode-mobile/packages/plugin-sdk-mobile/dev-shell` 存在） |
| Step 7 文档 | ⬜ | CONTEXT.md 术语段 + 2 个 ADR |
| Step 8 端到端验证 | ⬜ | 授权四路径 + 聊天 + 落盘 + 恢复 |

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
