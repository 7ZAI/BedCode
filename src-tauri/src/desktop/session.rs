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
//! - unified_output_queue.rs: 统一输出队列（环形缓冲区）
//! - session_output_manager.rs: 会话输出管理（订阅者管理）
//! - global_output_manager.rs: 全局输出管理（单例）

pub mod session_config;
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

// 输出管理模块
mod unified_output_queue;
mod session_output_manager;
mod global_output_manager;

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

// 输出管理模块导出
pub use unified_output_queue::{OutputEvent, UnifiedOutputQueue};
pub use session_output_manager::{SessionOutputManager, SubscriberState, SubscribeResponse};
pub use global_output_manager::GlobalOutputManager;

// 修改：从 desktop::model 导入，而非 types
pub use crate::desktop::model::{SessionInfo, SessionRestartEvent, SessionStatusEvent};

// Re-export from shared module
pub use crate::shared::enums::{SessionStatus, SessionType};