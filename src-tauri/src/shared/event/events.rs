//! Global Event System
//!
//! 项目全局事件顶层抽象
//! 所有模块的事件都应实现此 trait

use std::fmt::Debug;

/// 全局事件顶层 trait
/// 项目中所有事件类型都应实现此 trait
pub trait AppEvent: Clone + Send + Sync + Debug {}

/// 事件类别枚举
/// 用于事件分类和过滤
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCategory {
    /// 服务器事件
    Server,
    /// 客户端事件
    Client,
    /// 会话事件
    Session,
    /// 认证事件
    Auth,
    /// 数据库事件
    Database,
    /// 系统事件
    System,
    /// 网络事件
    Network,
    /// 通知事件
    Notification,
    /// 未知类别
    Unknown,
}

impl EventCategory {
    /// 从事件类型名称获取类别
    pub fn from_type_name(type_name: &str) -> Self {
        let lower = type_name.to_lowercase();
        if lower.contains("server") || lower.contains("websocket") {
            EventCategory::Server
        } else if lower.contains("client") {
            EventCategory::Client
        } else if lower.contains("session") {
            EventCategory::Session
        } else if lower.contains("auth") || lower.contains("pairing") {
            EventCategory::Auth
        } else if lower.contains("db") || lower.contains("database") {
            EventCategory::Database
        } else if lower.contains("system") {
            EventCategory::System
        } else if lower.contains("network") || lower.contains("connection") {
            EventCategory::Network
        } else if lower.contains("notify") {
            EventCategory::Notification
        } else {
            EventCategory::Unknown
        }
    }
}

/// 事件严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLevel {
    /// 调试级别
    Debug,
    /// 信息级别
    Info,
    /// 警告级别
    Warning,
    /// 错误级别
    Error,
    /// 严重错误级别
    Critical,
}

impl EventLevel {
    /// 从事件获取建议的日志级别
    pub fn from_event<E: AppEvent>(event: &E) -> Self {
        let type_name = std::any::type_name::<E>();
        let lower = type_name.to_lowercase();

        if lower.contains("error") || lower.contains("failed") {
            EventLevel::Error
        } else if lower.contains("timeout") || lower.contains("disconnected") {
            EventLevel::Warning
        } else if lower.contains("success") || lower.contains("connected") {
            EventLevel::Info
        } else {
            EventLevel::Debug
        }
    }
}

/// 事件构建器 trait
/// 用于创建标准化的事件实例
pub trait EventBuilder<E: AppEvent>: Send + Sync {
    /// 创建事件实例
    fn build(&self) -> E;
}

/// 事件发布器 trait
/// 用于事件订阅和发布
pub trait EventPublisher<E: AppEvent>: Send + Sync {
    /// 订阅事件，返回接收器
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<E>;

    /// 发布事件
    fn publish(&self, event: E) -> Result<(), tokio::sync::broadcast::error::SendError<E>>;
}