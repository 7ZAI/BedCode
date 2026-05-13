//! Session Management
//!
//! 提供会话状态管理、持久化和恢复功能
//!
//! 模块划分:
//! - mod.rs: 模块入口和导出
//! - types.rs: 公共类型定义
//! - session_manager.rs: SessionManager 主类实现
//! - storage.rs: SessionStorage 存储实现
//! - pty_handler.rs: PtySessionHandler 处理器实现

mod pty_handler;
mod session_manager;
mod storage;
mod types;

pub use pty_handler::{PtyHandler, PtySessionHandler};
pub use session_manager::SessionManager;
pub use storage::{SessionStore, SessionStorage};
pub use types::{SessionInfo, SessionRestartEvent, SessionStatus, SessionStatusEvent, SessionType};