# 12 — ai-chatbox plugin.json 补 `preauthUrls` 声明（预设 provider 域名）

**What to build:** `bedcode-mobile/plugins/ai-chatbox/plugin.json` 新增 `preauthUrls` 声明：预设 provider 域名（openai/deepseek/qwen/anthropic/dashscope 等）+ 允许路径粒度（glob，host + 可选 path 前缀）；用户自定义 baseUrl 不声明（走 L3 弹窗，ticket 10）。声明经 ticket 05 的宿主收集进 Egress L2。

**Spec:** §5.6 机制要点 2/4、§9 D6/D9、spec §4 插件段

**Blocked by:** 05

**Status:** ready-for-agent

## 关键实现事实（handoff §2/§3 已核实）

- ai-chatbox 请求链：前端 buildStreamRequest → `context.commands.execute('ai-chatbox.chat-stream')` → WASM Rust → host `http_fetch`（ticket 06 后过 Egress）。
- 预设 provider 域名：openai/deepseek/qwen/anthropic/dashscope；用户自定义 baseUrl 场景走 L3 弹窗（自定义 URL 未声明 → 弹窗授权）。
- 声明格式：`preauthUrls: string[]`（glob host+path，仿 preauthDirs 先例；字段解析由 ticket 05 提供）。

## 实现清单

- [ ] `plugin.json` 补 `preauthUrls`（预设 provider 域名 + 路径粒度 glob）
- [ ] 与 SDK manifest 类型对齐（ticket 05 同步后 build 验证）
- [ ] 插件前端 devMock/示例（如有）同步；`pnpm run build`（插件 + SDK）通过

## 验证

- 插件构建通过；宿主加载后 egress L2 含 ai-chatbox 声明域名
- 真机：预设 provider 请求无弹窗直放行；自定义 baseUrl 首次请求弹窗（验收 5）

## Comments

## Comments
- 2026-09-12 完成：ai-chatbox plugin.json `preauthUrls` 声明 5 条（glob `[scheme://][*.]host[:port][/path-prefix]`，与 SDK `PluginManifest.preauth_urls` camelCase 对齐）：`https://api.deepseek.com/*`、`https://dashscope.aliyuncs.com/*`（通义千问 compatible-mode）、`https://*.openai.com/*`（通配覆盖 api.openai.com 与 openai.com，去掉了冗余精确条目）、`https://api.anthropic.com/*`、`https://generativelanguage.googleapis.com/*`（Gemini adapter）。
- 域名来源核实：ai-chatbox `src/types.ts` 预设 provider baseUrl（deepseek v1 / dashscope compatible-mode / api.openai.com / api.anthropic.com）+ gemini adapter 测试用的 generativelanguage.googleapis.com。
- 用户自定义 baseUrl 不声明 → 走 L3 弹窗（ticket 10）。插件 devMock 无需同步（baseUrl 已与声明一致）。
- 宿主收集链路（ticket 05 已就绪）：manager.rs scan_and_load `register_plugin_urls`（加载即注册）→ egress L2 判定。cargo test 297 全绿（含 manifest 解析用例）。
