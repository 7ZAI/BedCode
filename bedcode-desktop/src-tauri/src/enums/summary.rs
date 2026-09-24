//! Summary Types
//!
//! 摘要类型定义 — 已迁移到 SDK `bedcode-plugin-api::wire::summary`（线协议单一
//! 事实来源），此处 re-export 保持宿主侧导入路径不变。
//!
//! 注意：会话**状态/类型**的 wire 形状在宿主 `protocol/session.rs`（票 08 归位），
//! 本文件的 `SessionSummary.status` 是对外**字符串**字段，由插件产出口自行折算。

pub use bedcode_plugin_api::wire::summary::SessionSummary;
