# 06 — 生命周期上报：mark_plugin_error（Rust/WASM 侧）

Type: task
Status: resolved
Blocked by: 02, 04

## 问题

用户要求：插件启用时应能通过生命周期函数上报启动成功/失败。
移动端当前 activate 成功/失败由宿主单方面依据 `__bedcode_activate` 返回码判定，
插件异步初始化失败（如 AI provider 配置无效）无上报通道 —— 状态会停留在 Activated。

桌面端 ABI v4 已有 `host_mark_plugin_error`：插件自检失败时上报宿主，
宿主置 Error + 持久化未启用 + 通知前端弹窗。

## 任务

1. `host/log.rs`：`HostLog::mark_plugin_error(&self, error: &str)`（桌面端语义一致）。
2. `abi.rs`：`import::MARK_PLUGIN_ERROR = "host_mark_plugin_error"` + 签名表 `(2, 0)`。
3. `wasm_host.rs`：`extern "C" fn host_mark_plugin_error(ptr, len)` + `WasmHost::mark_plugin_error`。
4. 成功上报：activate 返回 `Ok(())` 即成功（现有机制），无需额外通道；文档注明。

## Answer

## Answer

HostLog 增加 `mark_plugin_error(&self, error: &str)`（T2），WasmHost 已实现（T3），
extern 声明 host_mark_plugin_error 已加入（T3），abi.rs import::MARK_PLUGIN_ERROR + 签名表 (2,0)（T4）。
插件可调用 `host.mark_plugin_error("...")` 上报失败；成功由 activate 返回 Ok(()) 表示。
cargo test 通过。

## 验收

- SDK `cargo test` 通过。
- 插件代码可调用 `host.mark_plugin_error("...")`。
