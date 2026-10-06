# 03: 引擎面随迁（db / pty / enums / system process+opener）

**What to build:** 见 spec §6（行内）与对应 spec 章节。

**Status:** done（与 02 合并执行——编译闭包不可分割，见 issues/01 gate 裁定）

## Comments

- 2026-10-06 实施：M2（db 387+schema 73）、M3（pty 2,469）、M4（enums 33）、M5/M6（system/process 22 + system/opener 378）全部 `git mv` 入 crate，随 M1 一并落地。`schema.sql` 真源随迁（crate `src/db/schema.sql`，`include_str!` 相对路径不变仍可读）。
- lib 侧 `system.rs` 改为 `pub use bedcode_wasm_core::system::{config, opener, process};` 垫片；`db.rs` / `pty.rs` / `enums.rs` 删除，lib.rs 收 `pub use bedcode_wasm_core::{db, enums, pty};`。
- crate `system.rs` 复导出 `bedcode_server_base::{constants, error, error_boundary}` **整模块**（`crate::system::error::*` 等引用逐字不变）。

## Blocked by
- 01（已完成）

## 验证
- db 幂等迁移测试随迁后绿（`host_api::database::tests::*` / db 锁在 crate 全量绿）
- `retired_tables_are_not_created` 等 db 锁随迁后仍绿（crate `cargo test --lib` 含 db 域 40 项全绿）
