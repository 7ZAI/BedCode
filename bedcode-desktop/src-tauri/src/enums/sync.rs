//! Sync Types
//!
//! 数据同步相关类型 — 定义已迁移到 SDK `bedcode-plugin-api::wire::sync`
//! （线协议单一事实来源，会话引擎下沉专项票 01），此处 re-export 保持宿主侧
//! 导入路径不变。形状锁用例随定义迁到 SDK；宿主侧经
//! `server/websocket/message.rs` 的 `Message::SyncData` 往返套件依赖该锁。

pub use bedcode_plugin_api::wire::sync::SyncPayload;
