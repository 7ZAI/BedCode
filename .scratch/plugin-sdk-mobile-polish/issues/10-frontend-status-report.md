# 10 — 前端生命周期上报 API（启用成功/失败）

Type: task
Status: resolved
Blocked by: —

## 问题

前端 TS 插件激活时，loader 只能凭 `activate()` 的 resolve/reject 判定成败。
插件激活后异步初始化失败（如配置校验）无上报通道 —— 状态停留在 Activated。
用户要求：启用时通过生命周期函数上报启动成功/失败。

## 任务

1. SDK `src/types.ts`：
   - `PluginStatusAPI { reportReady(): Promise<void>; reportError(error: string): Promise<void> }`
   - `PluginContext` 增加 `readonly status: PluginStatusAPI`
2. 宿主 `src/plugin/context.ts` 实现 `status`：
   - `reportReady()`：插件主动声明启动成功（显式语义，区别于 activate 隐式成功）
   - `reportError(msg)`：调 `pluginCmds.pluginMarkError` + 通知宿主
3. 宿主 `loader.ts`：
   - `activate()` 成功时若插件声明了 `onStartup` 生命周期，等待其 reportReady 或超时（现有 IMPORT_TIMEOUT 复用）
   - `reportError` 后 loader 触发 deactivate 清理流程
4. 与 Rust/WASM 侧 `mark_plugin_error`（T6/T7）语义对齐：宿主收到任一侧上报，状态 → Error + 持久化未启用。

## Answer

## Answer

1. SDK types.ts 新增 StatusAPI（reportReady / reportError），PluginContext 增加 status。
2. context.ts 实现：reportError → pluginMarkError（置 Error）；reportReady → 新 command plugin_report_ready。
3. Rust 新增 plugin_report_ready command + PluginManager::report_ready（Error → Activated 自愈，
   Loaded → Activated 兜底，已激活不动），lib.rs invoke_handler 注册。
4. 前端 commands.ts 新增 pluginReportReady。
5. 与 Rust/WASM 侧 mark_plugin_error 语义对齐：任一侧上报失败 → 宿主置 Error + 持久化未启用。

## 验收

- `npm run test:run` 通过。
- 插件 `context.status.reportError('config invalid')` 后，前端显示 Error 状态并触发清理。
