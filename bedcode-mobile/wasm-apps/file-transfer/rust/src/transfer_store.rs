//! 传输任务台账 —— **实现已迁双端共享核**（`bedcode-file-transfer-core::transfer`，ADR 0044）。
//!
//! 本文件只剩转出：台账全是纯函数与领域类型（无宿主 I/O），故持久化与事件订阅不在这里，
//! 而在端内编排层（`peer.rs` 的 `load_entries` / `persist_entries`，走移动 storage 键）。
//!
//! 原实现与用例**逐字搬入**共享核（搬运等价性由
//! `.scratch/2026-10-09-file-transfer-shared-core/equiv-check-transfer.py` 逐函数校验，
//! 核内用例为两端并集），故本文件不再保留用例——保留会让同一份判据有两个测试真源。

pub(crate) use bedcode_file_transfer_core::transfer::*;
