//! 跨端线协议类型（单一事实源）
//!
//! 宿主 `bedcode-desktop/src-tauri/src/enums/` 与移动端
//! `bedcode-mobile/src-tauri/src/enums/` 里的会话同步 / WS 控制形状收编到此处：
//! 改字段名只改本模块一处，宿主与插件编译期同步（会话引擎下沉专项票 01）。
//!
//! **口径边界**（ADR 0022 裁剪线）：
//! - 本模块只定义**形状**，不含业务解释——宿主拿到 `SyncPayload` 后只封装成
//!   `Message::SyncData` 广播，不按变体分支决定推送语义。
//! - 认证线协议（`enums/auth.rs`）与会话记录 / 状态机线协议
//!   （宿主 `protocol/session.rs`）不在本模块：前者另线演进，后者是宿主传输面
//!   自有形状。
//! - 移动端保留平行副本（双端契约分叉先例，ADR 0018/0019 口径），由
//!   [`sync`] 的 `mobile_parallel_copy_shape_lock` 逐变体钉住与真源一致，
//!   不要求移动端直接依赖桌面 SDK crate。

pub mod control;
pub mod key;
pub mod summary;
pub mod sync;

pub use control::{SessionControlAction, SessionControlPayload, TerminalAction, TerminalPayload};
pub use key::{KeyCode, KeyCombo};
pub use summary::SessionSummary;
pub use sync::SyncPayload;
