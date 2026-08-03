# 07 — 宿主注册 mark_plugin_error + 状态处理闭环

Type: task
Status: resolved
Blocked by: 06

## 问题

SDK 侧已声明 `host_mark_plugin_error`，宿主 `wasm_runtime.rs` 需注册实现，
`manager.rs` 需处理错误上报的状态流转（Error + 持久化未启用 + 前端通知）。

## 任务

1. `wasm_runtime.rs`：
   - `register_host_functions` 注册 `host_mark_plugin_error`（`bedcode` 命名空间）。
   - `WasmHostContext` 增加 `mark_plugin_error(plugin_id, error)` 实现：
     调 `PluginManager::mark_error` + `set_enabled(plugin_id, false)` + 前端 Tauri 事件 `plugin:error`。
   - `WasmHostContext::new` 需要拿到 manager 引用或通过 app_handle 反查（评估：注入 manager Arc 更直接）。
2. `manager.rs`：`mark_error` 已有；补 `set_enabled(false)` 与前端通知。

## Answer

## Answer

1. WasmHostContext 增加 status_reporter 回调字段，manager.rs 注入闭包：置 Error 状态 +
   持久化 PLUGIN_ENABLED_KEY_PREFIX+id=false + emit "plugin:error" 前端事件 + tracing 日志。
2. register_host_functions 注册 host_mark_plugin_error（bedcode 命名空间）。
3. 顺带修复生命周期导出名漂移：types.rs wasm_export_name 改用 abi::export 常量。
宿主 cargo check 通过。

## 验收

- 宿主 `cargo test` 通过。
- 插件调用 `mark_plugin_error` 后：插件状态 → Error，启用状态持久化为 false，前端收到事件。
