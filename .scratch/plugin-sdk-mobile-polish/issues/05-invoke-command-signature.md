# 05 — 统一 WasmPlugin::invoke_command 签名

Type: task
Status: resolved
Blocked by: 01

## 问题

桌面端 `WasmPlugin::invoke_command(name: &str, args: serde_json::Value)`，
移动端 `invoke_command(name: &str, args_json: &str)` —— 签名不一致，插件无法跨端复用。
移动端插件（ai-chatbox）已按字符串版实现。

## 任务

1. `wasm.rs`：`WasmPlugin::invoke_command` 改为 `(name: &str, args: serde_json::Value)`。
   JSON 解析失败时传 `Value::Null`（与桌面端一致，宏负责解析）。
2. `wasm_entry!` 宏：`__bedcode_invoke_command` 内解析 args 字符串为 `Value` 再调用 trait 方法。
3. 宿主 `wasm_runtime.rs` 无需改动（线协议仍是 JSON 字符串，仅在宏内转换）。
4. 迁移插件：ai-chatbox 的 `invoke_command` 与 `commands::*` 改为接收 `Value`，用 `CommandArgs` 解析。
5. `BedcodePlugin`（Rust 原生插件）签名保持不动（`PluginCommand` handler 已是 `Value`）。

## Answer

## Answer

`WasmPlugin::invoke_command` 签名统一为 `(name: &str, args: serde_json::Value)`，
wasm_entry! 宏内解析 args JSON（失败传 Value::Null，与桌面端一致）。线协议仍为 JSON 字符串（v1），
宿主 wasm_runtime 无需改动。ai-chatbox / auto-task 已迁移（commands::* 改收 Value + CommandArgs 解析）。
两个插件 cargo check --features wasm 通过。

## 验收

- SDK `cargo test` 通过。
- ai-chatbox 编译通过且命令行为不变。
