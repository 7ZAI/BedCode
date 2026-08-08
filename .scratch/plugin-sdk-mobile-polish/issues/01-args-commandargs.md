# 01 — 移植 CommandArgs（args.rs）

Type: task
Status: claimed
Blocked by: —

## 问题

移动端 SDK 缺 `args.rs`，插件命令参数解析需要手写 `args.get("x").and_then(...)` 样板。
桌面端已有 `CommandArgs`（`str_or` / `str` / `bool_or` / `value` / `value_owned`）。

## 任务

1. 新建 `packages/plugin-sdk-mobile/rust/src/args.rs`，内容与桌面端 `plugin-sdk-desktop/rust/src/args.rs` 一致。
2. 在 `lib.rs` 注册 `pub mod args;` + `pub use args::CommandArgs;`。
3. 与 `WasmPlugin::invoke_command(name, args)` 签名统一后，插件侧用 `CommandArgs::new(args)` 解析。

## Answer

已新建 `packages/plugin-sdk-mobile/rust/src/args.rs`（含 2 个单元测试）并注册到 `lib.rs`。
`cargo test` 通过（2 passed）。API 与桌面端一致：`CommandArgs::new` / `str_or` / `str` / `bool_or` / `value` / `value_owned`，`Null` 归一化为空对象。

Status: resolved
