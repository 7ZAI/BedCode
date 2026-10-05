//! `host-database` / `host-plugin-database` 两域的用例入口（宿主 `wasm_core` 内）
//!
//! 分组：主库隔离纵深（`db_isolation`）· 权限门三态（`db_faces`）· SQL 前缀校验纯函数
//! （`sql_parse`）· 执行护栏（`db_guards`）· 事务批次（`db_batch`）。
//!
//! 用例文件经 `use crate::wasm_core::host_api::sqlite_scaffold::*` 取假端口（跨分组
//! 共享脚手架，见该文件模块头）；`host-storage` 的 kv 用例在 `super::super::storage::tests`。

mod db_batch;
mod db_faces;
mod db_guards;
mod db_isolation;
mod sql_parse;
