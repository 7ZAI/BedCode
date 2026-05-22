//! Router Usage Example
//!
//! 展示如何在项目中使用新的消息路由器
//!
//! ```rust
//! use crate::desktop::server::router::{
//!     BusinessRouter, MessageType, middlewares::*,
//!     handlers::{AuthHandler, ControlHandler, InputHandler, SubscribeHandler, HeartbeatHandler},
//! };
//! use std::sync::Arc;
//!
//! // 1. 创建处理器
//! let auth_handler = Arc::new(AuthHandler::new(
//!     db.clone(),
//!     pairing_service.clone(),
//!     qr_manager.clone(),
//! ));
//!
//! let control_handler = Arc::new(ControlHandler::new(
//!     session_manager.clone(),
//!     plugin_manager.clone(),
//!     db.clone(),
//! ));
//!
//! let input_handler = Arc::new(InputHandler::new(session_manager.clone()));
//! let subscribe_handler = Arc::new(SubscribeHandler::new());
//! let heartbeat_handler = Arc::new(HeartbeatHandler::new());
//!
//! // 2. 构建路由器
//! let router = BusinessRouter::builder()
//!     .route(MessageType::Auth, auth_handler)
//!     .route(MessageType::Control, control_handler)
//!     .route(MessageType::Input, input_handler)
//!     .route(MessageType::Subscribe, subscribe_handler.clone())
//!     .route(MessageType::Unsubscribe, subscribe_handler)
//!     .route(MessageType::Heartbeat, heartbeat_handler)
//!     .middleware(Arc::new(LoggingMiddleware))
//!     .middleware(Arc::new(AuthMiddleware))
//!     .build();
//!
//! // 3. 创建适配器并注入到 WebSocketManager
//! let adapter = Arc::new(RouterAdapter::new(
//!     Arc::new(router),
//!     connection_manager,
//!     event_tx,
//! ));
//!
//! // 4. 初始化 WebSocketManager
//! WebSocketManager::global().init(Some(adapter)).await?;
//! ```

/// 创建并配置路由器的便捷函数
#[allow(dead_code)]
pub fn create_router(
    db: std::sync::Arc<tokio::sync::Mutex<crate::shared::db::Database>>,
    pairing_service: std::sync::Arc<crate::desktop::server::services::PairingService>,
    qr_manager: std::sync::Arc<crate::shared::auth::QrTokenManager>,
    session_manager: Option<std::sync::Arc<crate::desktop::session::SessionManager>>,
    plugin_manager: Option<std::sync::Arc<crate::desktop::plugin::PluginManager>>,
) -> crate::desktop::server::router::BusinessRouter {
    use crate::desktop::server::router::{BusinessRouter, MessageType};
    use crate::desktop::server::handlers::{
        AuthHandler, ControlHandler, HeartbeatHandler, InputHandler, SubscribeHandler,
    };
    use std::sync::Arc;
    use crate::desktop::server::router::middlewares::{AuthMiddleware, LoggingMiddleware};

    let auth_handler = Arc::new(AuthHandler::new(
        db.clone(),
        pairing_service.clone(),
        qr_manager.clone(),
    ));

    let control_handler = Arc::new(ControlHandler::new(
        session_manager.clone(),
        plugin_manager.clone(),
        db.clone(),
    ));

    let input_handler = Arc::new(InputHandler::new(session_manager.clone()));
    let subscribe_handler = Arc::new(SubscribeHandler::new());
    let heartbeat_handler = Arc::new(HeartbeatHandler::new());

    BusinessRouter::builder()
        .route(MessageType::Auth, auth_handler)
        .route(MessageType::Control, control_handler)
        .route(MessageType::Input, input_handler)
        .route(MessageType::Subscribe, subscribe_handler.clone())
        .route(MessageType::Unsubscribe, subscribe_handler)
        .route(MessageType::Heartbeat, heartbeat_handler)
        .middleware(Arc::new(LoggingMiddleware))
        .middleware(Arc::new(AuthMiddleware))
        .build()
}