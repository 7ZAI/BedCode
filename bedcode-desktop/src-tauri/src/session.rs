//! Session Management
//!
//! 提供会话状态管理、持久化和恢复功能
//!
//! **对外协议形状不在本目录**（票 02）：`SessionInfo` / `SessionInfoView` /
//! `ResizeOutcome` / `RendererSource` 已迁 `crate::protocol::session`——本目录是
//! 退役中的内核会话线，协议形状必须活过它的删除。
//!
//! 模块划分:
//! - session_config.rs: 会话配置管理（CRUD 操作）
//! - session_manager.rs: SessionManager 主类实现
//! - session_components.rs: 内部组件
//! - session_output.rs: 输出管理

pub mod session_config;
pub mod session_event;
pub mod session_lifecycle;
mod session_manager;

mod session_components;
mod session_output;

pub use session_config::SessionConfigManager;
pub use session_event::SessionStatusEvent;
pub use session_manager::SessionManager;

// 从 session_components 重导出
pub use session_components::{
    resolve_initial_size, CanonicalRendererRegistry, DefaultCanonicalRendererRegistry, DefaultPtyRegistry,
    DefaultSessionInfoRegistry, PtyRegistry, SessionInfoRegistry,
};

// 从 session_output 重导出
pub use session_output::{
    GlobalOutputManager, OutputEvent, PullSubscriber, RingFetchOutput, RingSlice, SessionOutputManager, SessionOutputSink,
    SubscribeResponse, SubscriberHandle, SubscriberStats, UnifiedOutputQueue, MODE_BATCH, MODE_REALTIME,
};

pub use session_lifecycle::{SessionLifecycleEvent, SessionLifecycleListener};

// `input_line`（提交输入行重建 + 观察扩展点，ADR 0001）随票 03 删除：
// 宿主侧观察面退役，「注册了就能收到提交行」的注册表与派发点同批移除；
// 行重建真源在 `com.bedcode.terminal-session` 的 `session/input_line.rs`。

// Re-export from enums
pub use crate::enums::{SessionStatus, SessionType};
