//! 传输任务台账 —— **实现已迁双端共享核**（`bedcode-file-transfer-core::transfer`，ADR 0044）。
//!
//! 本文件只剩转出：台账全是纯函数与领域类型（无宿主 I/O），故持久化与事件订阅不在这里，
//! 而在端内编排层（`peer.rs` 的 `load_entries` / `persist_entries`，走 `host-plugin-database`
//! 独立表 `transfer_entries`）。
//!
//! **本端行为随迁移发生的变化（用户裁决 T7=B，桌面统一到修正版）**：
//! 事件归约新增 `pull-started` 建行锚点——本端拉取批次由引擎事件即建行，不再依赖旧快照
//! merge 补行。依据：引擎侧 `pull-started` 由两端共用的 `bedcode-server-peer-net` 发出，
//! 其注释明写「任务行由插件归约自建」；且本端自有文档即写明「事件归约是 store 主写、
//! 快照 merge 退化为校正」。旧快照通路（`merge_snapshot` / `prune_absent` /
//! `reconcile_diff`）**保留不变**，继续承担对账与校正。
//!
//! 原实现与用例**逐字搬入**共享核（搬运等价性由
//! `.scratch/2026-10-09-file-transfer-shared-core/equiv-check-transfer.py` 逐函数校验，
//! 核内用例为两端并集），故本文件不再保留用例——保留会让同一份判据有两个测试真源。

pub(crate) use bedcode_file_transfer_core::transfer::*;
