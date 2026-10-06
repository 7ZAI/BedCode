# 02: 骨架 crate + wasm_core 机制本体机械搬迁（tracer bullet）

**What to build:** 见 spec §6（行内）与对应 spec 章节。

**Status:** done（与 03/04 合并执行——票 01 gate 已裁定编译闭包不可分割，见 issues/01）

## Comments

- 2026-10-06 实施：`bedcode-desktop/packages/bedcode-wasm-core/` 建成。M1（wasm_core 全目录 119 文件）+ lib.rs 垫片（`pub use bedcode_wasm_core as wasm_core;` + `db/enums/pty`）+ `runtime_util` pub 化 + `bindgen!` 路径改 `../plugin-sdk-desktop/rust/wit/bedcode.wit` + Cargo.toml（依赖对齐 src-tauri 锁步）。crate 根 `cargo check --lib` 绿。
- 为满足「每票后各自可编译」，实际执行顺序：M1 + M2-M7（db/pty/enums/system{config,opener,process}）+ M8-M11（auth_center/session_gateway/test_tokens/HostBusPort）+ 注册表 + PeerCtxProvider 端口 + harness 一次性落地（编译闭包不可分，gate 裁定）。03/04 的票面内容已包含在本票执行中。
- **行数量测（票 01 修正后）**：M1 = 54,394（119 文件）；本票实际 move 总量约 59,175 行。

## Blocked by
- 01（gate，已完成）

## 验证
- crate `cargo check --lib` 绿（0 error）
- src-tauri `cargo check` / `cargo check --tests` 绿（垫片生效）
- cross-end-tests `cargo check --tests` 绿
