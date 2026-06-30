//! Session Management
//!
//! 提供会话状态管理、持久化和恢复功能
//!
//! 模块划分:
//! - session_config.rs: 会话配置管理（CRUD 操作）
//! - session_manager.rs: SessionManager 主类实现
//! - storage.rs: SessionStorage 存储实现
//! - event_bus.rs: 统一事件广播
//! - session_components.rs: 内部组件（PtyRegistry, SessionInfoRegistry, NamingService, ConfigMapper, StatusDetector）
//! - session_output.rs: 输出管理（OutputCache, UnifiedOutputQueue, SessionOutputManager, GlobalOutputManager）

pub mod session_config;
mod session_manager;
mod storage;

mod event_bus;
mod session_components;
mod session_output;

pub use session_config::SessionConfigManager;
pub use session_manager::SessionManager;
pub use storage::{SessionStore, SessionStorage};

// 从 session_components 重导出
pub use session_components::{
    DefaultPtyRegistry, PtyRegistry,
    DefaultSessionInfoRegistry, SessionInfoRegistry,
    DefaultNamingService, NamingService,
    DefaultConfigMapper, ConfigMapper,
    DefaultStatusDetector, StatusDetector,
};

// 从 event_bus 重导出
pub use event_bus::{DefaultSessionEventBus, SessionEventBus, SessionEvent};

// 从 session_output 重导出
pub use session_output::{
    DefaultOutputCache, OutputCache,
    OutputEvent, UnifiedOutputQueue,
    SessionOutputManager, SubscriberState, SubscribeResponse,
    GlobalOutputManager,
};

// 从 desktop::model 导入
pub use crate::desktop::model::{SessionInfo, SessionRestartEvent, SessionStatusEvent};

// Re-export from shared module
pub use crate::shared::enums::{SessionStatus, SessionType};
