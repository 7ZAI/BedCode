//! Session Management
//!
//! 提供会话状态管理、持久化和恢复功能
//!
//! 模块划分:
//! - session_config.rs: 会话配置管理（CRUD 操作）
//! - session_manager.rs: SessionManager 主类实现
//! - storage.rs: SessionStorage 存储实现
//! - pty_registry.rs: PTY 会话注册表
//! - session_info.rs: 会话信息注册表
//! - output_cache.rs: PTY 输出缓存
//! - event_bus.rs: 统一事件广播
//! - naming_service.rs: 命名服务
//! - config_mapper.rs: 配置映射服务
//! - status_detector.rs: 状态检测服务

mod session_config;
mod session_manager;
mod storage;

// 新模块
mod pty_registry;
mod session_info;
mod output_cache;
mod event_bus;
mod naming_service;
mod config_mapper;
mod status_detector;

// 删除: mod types; (已移动到 model/session_event.rs)

pub use session_config::SessionConfigManager;
pub use session_manager::SessionManager;
pub use storage::{SessionStore, SessionStorage};

pub use pty_registry::{DefaultPtyRegistry, PtyRegistry};
pub use session_info::{DefaultSessionInfoRegistry, SessionInfoRegistry};
pub use output_cache::{DefaultOutputCache, OutputCache};
pub use event_bus::{DefaultSessionEventBus, SessionEventBus, SessionEvent};
pub use naming_service::{DefaultNamingService, NamingService};
pub use config_mapper::{DefaultConfigMapper, ConfigMapper};
pub use status_detector::{DefaultStatusDetector, StatusDetector};

// 修改：从 desktop::model 导入，而非 types
pub use crate::desktop::model::{SessionInfo, SessionRestartEvent, SessionStatusEvent};

// Re-export from shared module
pub use crate::shared::enums::{SessionStatus, SessionType};