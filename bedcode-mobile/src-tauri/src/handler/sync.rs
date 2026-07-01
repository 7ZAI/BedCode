//! Sync Handler - 同步数据消息处理器

use async_trait::async_trait;

use crate::model::message::Message;
use crate::enums::SyncPayload;
use crate::Result;

use crate::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 同步数据消息处理器
pub struct SyncHandler;

#[async_trait]
impl ClientRouteHandler for SyncHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::SyncData { payload, .. } = message {
            match payload {
                SyncPayload::SessionCreated { session, source_device } => {
                    tracing::info!("[SyncHandler] SessionCreated: session_id={}, source={}", session.id, source_device);
                    ctx.emit(MobileEvent::SyncSessionCreated {
                        session,
                        source_device,
                    });
                }
                SyncPayload::SessionStatusChanged { session_id, old_status, new_status, session_name } => {
                    tracing::info!("[SyncHandler] SessionStatusChanged: session_id={}, {} -> {}", session_id, old_status, new_status);
                    ctx.emit(MobileEvent::SyncSessionStatusChanged {
                        session_id,
                        old_status,
                        new_status,
                        session_name,
                    });
                }
                SyncPayload::SessionStopped { session_id, session_name } => {
                    tracing::info!("[SyncHandler] SessionStopped: session_id={}", session_id);
                    ctx.emit(MobileEvent::SyncSessionStopped {
                        session_id,
                        session_name,
                    });
                }
                SyncPayload::SessionRemoved { session_id, session_name } => {
                    tracing::info!("[SyncHandler] SessionRemoved: session_id={}", session_id);
                    ctx.emit(MobileEvent::SyncSessionRemoved {
                        session_id,
                        session_name,
                    });
                }
                SyncPayload::ConfigCreated { config, source_device } => {
                    tracing::info!("[SyncHandler] ConfigCreated: config_id={}, source={}", config.id, source_device);
                    ctx.emit(MobileEvent::SyncConfigCreated {
                        config,
                        source_device,
                    });
                }
                SyncPayload::ConfigUpdated { config, source_device } => {
                    tracing::info!("[SyncHandler] ConfigUpdated: config_id={}, source={}", config.id, source_device);
                    ctx.emit(MobileEvent::SyncConfigUpdated {
                        config,
                        source_device,
                    });
                }
                SyncPayload::ConfigRemoved { config_id, config_name } => {
                    tracing::info!("[SyncHandler] ConfigRemoved: config_id={}", config_id);
                    ctx.emit(MobileEvent::SyncConfigRemoved {
                        config_id,
                        config_name,
                    });
                }
                SyncPayload::TaskStatusChanged { session_id, task_status, task_reason, task_questions } => {
                    tracing::info!("[SyncHandler] TaskStatusChanged: session_id={}, status={}", session_id, task_status);
                    ctx.emit(MobileEvent::SyncTaskStatusChanged {
                        session_id,
                        task_status,
                        task_reason,
                        task_questions,
                    });
                }
                SyncPayload::SessionModeChanged { session_id, auto_approve } => {
                    tracing::info!("[SyncHandler] SessionModeChanged: session_id={}, auto_approve={}", session_id, auto_approve);
                    ctx.emit(MobileEvent::SyncSessionModeChanged {
                        session_id,
                        auto_approve,
                    });
                }
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "SyncHandler"
    }
}

impl Default for SyncHandler {
    fn default() -> Self {
        Self
    }
}