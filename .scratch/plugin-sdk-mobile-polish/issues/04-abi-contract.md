# 04 — 新增 abi.rs（ABI 单一事实来源）

Type: task
Status: resolved
Blocked by: —

## 问题

移动端 WASM 契约（host function 名、导出函数名、签名）散落在 `wasm_host.rs` 的 `extern "C"`
声明与宿主 `wasm_runtime.rs` 的 `register_host_functions` 中，无单一来源、无签名表，
任何一侧改名/改参数都会静默漂移。

## 任务

1. 新建 `packages/plugin-sdk-mobile/rust/src/abi.rs`，参照桌面端：
   - `pub mod import { ... }`：host function 名称常量（移动端现有 21 个 + 新增 `host_mark_plugin_error`）
   - `pub mod export { ... }`：插件导出名称常量
   - `HOST_FN_SIGNATURES: &[(&str, usize, usize)]`：参数/返回值个数表
   - `PLUGIN_EXPORT_SIGNATURES`：插件导出签名表（移动端按现状：invoke_command 返回 2 值等）
   - `ABI_VERSION` / `NAMESPACE` / `MEMORY` / `RESULT_PAIR_SIZE`
2. `wasm_host.rs` 的 `extern "C"` 声明改用 `abi::import::*` 常量名。
3. `wasm_entry!` 宏的导出名改用 `abi::export::*` 常量名。

## Answer

## Answer

已新建 `rust/src/abi.rs`：NAMESPACE / MEMORY / ABI_VERSION=2 / export 名称常量（14 个）/
import 名称常量（22 个，含 host_mark_plugin_error）/ HOST_FN_SIGNATURES / PLUGIN_EXPORT_SIGNATURES。
v2 变更：新增 mark_plugin_error，修复宿主 on_app_startup/on_app_shutdown 导出名漂移
（宿主 wasm_export_name 已改用 abi 常量）。cargo test 通过。

## 验收

- SDK `cargo test` 通过。
- 宿主 `register_host_functions` 可遍历 `HOST_FN_SIGNATURES` 校验注册一致性（T8 落地）。
