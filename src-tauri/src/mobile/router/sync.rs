//! Sync Router - 同步数据消息路由处理器

use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::shared::enums::SyncPayload;
use crate::Result;

use super::{ClientRouteContext, ClientRouteHandler, MobileEvent};

/// 同步数据消息路由器
pub struct SyncRouter;

#[async_trait]
impl ClientRouteHandler for SyncRouter {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::SyncData { payload, .. } = message {
            match payload {
                SyncPayload::SessionCreated { session, source_device } => {
                    tracing::info!("[SyncRouter] SessionCreated: session_id={}, source={}", session.id, source_device);
                    ctx.emit(MobileEvent::SyncSessionCreated {
                        session,
                        source_device,
                    });
                }
                SyncPayload::SessionStatusChanged { session_id, old_status, new_status, session_name } => {
                    tracing::info!("[SyncRouter] SessionStatusChanged: session_id={}, {} -> {}", session_id, old_status, new_status);
                    ctx.emit(MobileEvent::SyncSessionStatusChanged {
                        session_id,
                        old_status,
                        new_status,
                        session_name,
                    });
                }
                SyncPayload::SessionStopped { session_id, session_name } => {
                    tracing::info!("[SyncRouter] SessionStopped: session_id={}", session_id);
                    ctx.emit(MobileEvent::SyncSessionStopped {
                        session_id,
                        session_name,
                    });
                }
                SyncPayload::SessionRemoved { session_id, session_name } => {
                    tracing::info!("[SyncRouter] SessionRemoved: session_id={}", session_id);
                    ctx.emit(MobileEvent::SyncSessionRemoved {
                        session_id,
                        session_name,
                    });
                }
                SyncPayload::ConfigCreated { config, source_device } => {
                    tracing::info!("[SyncRouter] ConfigCreated: config_id={}, source={}", config.id, source_device);
                    ctx.emit(MobileEvent::SyncConfigCreated {
                        config,
                        source_device,
                    });
                }
                SyncPayload::ConfigUpdated { config, source_device } => {
                    tracing::info!("[SyncRouter] ConfigUpdated: config_id={}, source={}", config.id, source_device);
                    ctx.emit(MobileEvent::SyncConfigUpdated {
                        config,
                        source_device,
                    });
                }
                SyncPayload::ConfigRemoved { config_id, config_name } => {
                    tracing::info!("[SyncRouter] ConfigRemoved: config_id={}", config_id);
                    ctx.emit(MobileEvent::SyncConfigRemoved {
                        config_id,
                        config_name,
                    });
                }
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "SyncRouter"
    }
}

impl Default for SyncRouter {
    fn default() -> Self {
        Self
    }
}
