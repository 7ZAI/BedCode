//! 插件 SQL 护栏与列转换 —— re-export 垫片（票 18 批次 3 + 主库收归）
//!
//! 原 `validate_sql_table_prefix` / `extract_table_names`（主库表名前缀纵深，票 17
//! 批次 2 自宿主 `plugin/wasm_host.rs` 迁入）随 **2026-10-09 主库收归 wasm-core**
//! （`host-database` 双端退役）删除；`column_to_json` 等实现层上移共享核
//! `bedcode-host-api-core::database`。本文件保留为垫片，保宿主
//! `crate::plugin::wasm_host` 的 `pub use ...::sql_guard::*` 转发面不空转；
//! 实际消费者已改走共享核（`host_impl/db.rs`）。

pub use bedcode_host_api_core::database::column_to_json;
