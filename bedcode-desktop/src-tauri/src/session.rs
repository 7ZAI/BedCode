//! Session Management
//!
//! 提供会话状态管理、持久化和恢复功能
//!
//! 模块划分:
//! - session_config.rs: 会话配置管理（CRUD 操作）
//! - session_manager.rs: SessionManager 主类实现
//! - event_bus.rs: 统一事件广播
//! - session_components.rs: 内部组件
//! - session_output.rs: 输出管理

pub mod session_config;
pub mod session_event;
pub mod session_lifecycle;
mod session_manager;

mod event_bus;
mod input_line;
mod session_components;
mod session_output;

pub use session_config::SessionConfigManager;
pub use session_event::{task_fields_from_slot, SessionInfo, SessionInfoView, SessionStatusEvent};
pub use session_manager::SessionManager;

// 从 session_components 重导出
pub use session_components::{
    resolve_initial_size, CanonicalRendererRegistry, DefaultCanonicalRendererRegistry, DefaultPtyRegistry,
    DefaultSessionInfoRegistry, PtyRegistry, RendererSource, ResizeOutcome, SessionInfoRegistry,
};

// 从 event_bus 重导出
pub use event_bus::{DefaultSessionEventBus, SessionEvent, SessionEventBus};

// 从 session_output 重导出
pub use session_output::{
    GlobalOutputManager, OutputEvent, PullSubscriber, RingSlice, SessionOutputManager, SubscribeResponse,
    SubscriberHandle, SubscriberStats, UnifiedOutputQueue, MODE_BATCH, MODE_REALTIME,
};

pub use session_lifecycle::{SessionLifecycleEvent, SessionLifecycleListener};

// 从 input_line 重导出（提交输入行观察扩展点，见 ADR 0001）
pub use input_line::{SessionInputListener, SubmittedLineTracker};

// Re-export from enums
pub use crate::enums::{SessionStatus, SessionType};
