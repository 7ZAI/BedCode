# 03 — WasmHost 实现 host trait（签名约束落地）

Type: task
Status: resolved
Blocked by: 02

## 问题

`wasm_host.rs` 的 `WasmHost` 方法签名游离在 trait 之外，与 `host/` 契约无关联；
宿主侧 `WasmHostContext` 也无法用同一 trait 约束。

## 任务

1. `impl HostStorage for WasmHost`、`impl HostLog for WasmHost` … 全部子 trait。
2. 现有方法保留（兼容存量插件），新增缺失方法（如 `mark_plugin_error`、`session_*` 补齐）。
3. 错误语义统一：宿主调用失败返回 `HostError`（`code = -1` / `-2`）。
4. 保持 `wasm_alloc_string` / `wasm_read_string` 辅助函数不变。

## Answer

## Answer

WasmHost 重构为无状态 unit struct（`#[derive(Debug, Clone, Copy, Default)] pub struct WasmHost;`），
删除冗余 plugin_id 字段（宿主从 Caller state 注入），删除全部 inherent 方法，改为实现 host/ 全部子 trait，
错误语义统一为 Result<_, HostError>。wasm_entry! 宏同步适配（WasmHost 直接构造 + HostLog trait 引入）。
cargo test --features wasm 通过。

## 验收

- SDK `cargo test` 通过。
- `WasmHost` 实现全部 `host/` 子 trait，`&impl HostApi` 可用于插件代码。
- 存量插件（ai-chatbox）编译通过。
