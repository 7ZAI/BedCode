//! Sync Event Handler
//!
//! 同步事件处理器，将 DesktopSyncEvent 转换为 SyncData WebSocket 消息并广播

use crate::events::DesktopSyncEvent;
use crate::session::SessionManager;
use crate::session::SessionConfigManager;
use crate::server::ws::WebSocketManager;
use crate::enums::{SessionConfigSummary, SessionSummary, SyncPayload};
use super::matcher::EventHandler;
use crate::server::ws::message::Message;
use std::sync::Arc;

/// 同步事件处理器
///
/// 将 DesktopSyncEvent 转换为 SyncData WebSocket 消息并广播给客户端
pub struct SyncEventHandler {
    session_manager: Arc<SessionManager>,
    config_manager: Arc<SessionConfigManager>,
    ws_manager: &'static WebSocketManager,
}

impl SyncEventHandler {
    /// 创建新的同步事件处理器
    pub fn new(
        session_manager: Arc<SessionManager>,
        config_manager: Arc<SessionConfigManager>,
        ws_manager: &'static WebSocketManager,
    ) -> Self {
        Self {
            session_manager,
            config_manager,
            ws_manager,
        }
    }

    /// 异步处理事件
    async fn process_event(&self, event: DesktopSyncEvent) {
        tracing::info!("[SyncEventHandler] Processing event: {:?}", event);
        match event {
            DesktopSyncEvent::SessionCreated { session_id, source_device } => {
                self.handle_session_created(&session_id, source_device).await;
            }
            DesktopSyncEvent::SessionStatusChanged { session_id, old_status, new_status } => {
                self.handle_session_status_changed(&session_id, old_status, new_status).await;
            }
            DesktopSyncEvent::SessionStopped { session_id, source_device } => {
                self.handle_session_stopped(&session_id, source_device).await;
            }
            DesktopSyncEvent::SessionRemoved { session_id, source_device } => {
                self.handle_session_removed(&session_id, source_device).await;
            }
            DesktopSyncEvent::ConfigCreated { config_id, source_device } => {
                self.handle_config_created(&config_id, source_device).await;
            }
            DesktopSyncEvent::ConfigUpdated { config_id, source_device } => {
                self.handle_config_updated(&config_id, source_device).await;
            }
            DesktopSyncEvent::ConfigRemoved { config_id, config_name, source_device } => {
                self.handle_config_removed(&config_id, &config_name, source_device).await;
            }
            DesktopSyncEvent::TaskStatusChanged { session_id, task_status, task_reason, task_questions } => {
                self.handle_task_status_changed(&session_id, &task_status, task_reason.as_deref(), task_questions.as_deref()).await;
            }
            DesktopSyncEvent::SessionModeChanged { session_id, auto_approve } => {
                self.handle_session_mode_changed(&session_id, auto_approve).await;
            }
        }
    }

    /// 处理会话创建事件
    async fn handle_session_created(&self, session_id: &str, source_device: Option<String>) {
        // 获取会话信息
        let Some(session_info) = self.session_manager.get_session(session_id).await else {
            tracing::warn!("[SyncEventHandler] Session not found: {}", session_id);
            return;
        };

        // 构建 SessionSummary
        let session = SessionSummary {
            id: session_info.id,
            name: session_info.name,
            status: format!("{:?}", session_info.status).to_lowercase(),
            created_at: session_info.created_at.to_rfc3339(),
            started_at: session_info.started_at.map(|t| t.to_rfc3339()),
            session_type: Some(format!("{:?}", session_info.session_type).to_lowercase()),
            config_id: Some(session_info.config_id),
            task_status: session_info.task_status.map(|ts| format!("{:?}", ts).to_lowercase()),
            task_reason: session_info.task_reason,
        };

        // 提取 source_device 值
        let source_device_str = source_device.clone().unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::SessionCreated {
            session,
            source_device: source_device_str.clone(),
        };

        // 广播消息
        self.broadcast_sync_data(payload, Some(&source_device_str)).await;
    }

    /// 处理会话状态变化事件
    async fn handle_session_status_changed(
        &self,
        session_id: &str,
        old_status: crate::enums::SessionStatus,
        new_status: crate::enums::SessionStatus,
    ) {
        // 获取会话名称
        let session_name = self.session_manager.get_session(session_id).await
            .map(|s| s.name)
            .unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::SessionStatusChanged {
            session_id: session_id.to_string(),
            old_status: format!("{:?}", old_status).to_lowercase(),
            new_status: format!("{:?}", new_status).to_lowercase(),
            session_name,
        };

        // 状态变化广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 处理会话停止事件
    async fn handle_session_stopped(&self, session_id: &str, source_device: Option<String>) {
        // 获取会话名称
        let session_name = self.session_manager.get_session(session_id).await
            .map(|s| s.name)
            .unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::SessionStopped {
            session_id: session_id.to_string(),
            session_name,
        };

        // 广播消息
        self.broadcast_sync_data(payload, source_device.as_deref()).await;
    }

    /// 处理会话删除事件
    async fn handle_session_removed(&self, session_id: &str, source_device: Option<String>) {
        // 注意：此时会话可能已从 SessionManager 移除，session_name 可能为空
        // 调用方应在移除前获取名称

        // 构建同步载荷
        let payload = SyncPayload::SessionRemoved {
            session_id: session_id.to_string(),
            session_name: String::new(), // 已删除，名称不可用
        };

        // 广播消息
        self.broadcast_sync_data(payload, source_device.as_deref()).await;
    }

    /// 处理配置创建事件
    async fn handle_config_created(&self, config_id: &str, source_device: Option<String>) {
        // 获取配置信息
        let Ok(Some(config)) = self.config_manager.get_config(config_id).await else {
            tracing::warn!("[SyncEventHandler] Config not found: {}", config_id);
            return;
        };

        // 构建 SessionConfigSummary
        let config_summary = SessionConfigSummary {
            id: config.id,
            name: config.name,
            environment: config.environment,
            wsl_distro: config.wsl_distro,
            working_dir: config.working_dir,
            command: config.command,
        };

        // 提取 source_device 值
        let source_device_str = source_device.clone().unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::ConfigCreated {
            config: config_summary,
            source_device: source_device_str.clone(),
        };

        // 广播消息
        self.broadcast_sync_data(payload, Some(&source_device_str)).await;
    }

    /// 处理配置更新事件
    async fn handle_config_updated(&self, config_id: &str, source_device: Option<String>) {
        // 获取配置信息
        let Ok(Some(config)) = self.config_manager.get_config(config_id).await else {
            tracing::warn!("[SyncEventHandler] Config not found: {}", config_id);
            return;
        };

        // 构建 SessionConfigSummary
        let config_summary = SessionConfigSummary {
            id: config.id,
            name: config.name,
            environment: config.environment,
            wsl_distro: config.wsl_distro,
            working_dir: config.working_dir,
            command: config.command,
        };

        // 提取 source_device 值
        let source_device_str = source_device.clone().unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::ConfigUpdated {
            config: config_summary,
            source_device: source_device_str.clone(),
        };

        // 广播消息
        self.broadcast_sync_data(payload, Some(&source_device_str)).await;
    }

    /// 处理配置删除事件
    async fn handle_config_removed(&self, config_id: &str, config_name: &str, source_device: Option<String>) {
        // 构建同步载荷
        let payload = SyncPayload::ConfigRemoved {
            config_id: config_id.to_string(),
            config_name: config_name.to_string(),
        };

        // 广播消息
        self.broadcast_sync_data(payload, source_device.as_deref()).await;
    }

    /// 处理任务状态变更事件
    async fn handle_task_status_changed(
        &self,
        session_id: &str,
        task_status: &str,
        task_reason: Option<&str>,
        task_questions: Option<&[crate::enums::PluginQuestion]>,
    ) {
        let payload = SyncPayload::TaskStatusChanged {
            session_id: session_id.to_string(),
            task_status: task_status.to_string(),
            task_reason: task_reason.map(|s| s.to_string()),
            task_questions: task_questions.map(|qs| qs.to_vec()),
        };

        // 任务状态变更广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 处理会话模式变更事件
    async fn handle_session_mode_changed(&self, session_id: &str, auto_approve: bool) {
        let payload = SyncPayload::SessionModeChanged {
            session_id: session_id.to_string(),
            auto_approve,
        };

        // 模式变更广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 广播同步数据消息
    ///
    /// 如果指定了 exclude_device，则排除该设备后广播给其他客户端
    /// 否则广播给所有已认证客户端
    async fn broadcast_sync_data(&self, payload: SyncPayload, exclude_device: Option<&str>) {
        let message = Message::sync_data(payload);

        if let Some(device_name) = exclude_device {
            if !device_name.is_empty() {
                // 排除操作者，广播给其他客户端
                if let Err(e) = self.ws_manager.broadcast_sync_to_others(device_name, &message).await {
                    tracing::error!("[SyncEventHandler] Failed to broadcast to others: {}", e);
                }
            } else {
                // 桌面本地操作，广播给所有客户端
                if let Err(e) = self.ws_manager.broadcast(&message).await {
                    tracing::error!("[SyncEventHandler] Failed to broadcast: {}", e);
                }
            }
        } else {
            // 状态变化等事件，广播给所有客户端
            if let Err(e) = self.ws_manager.broadcast(&message).await {
                tracing::error!("[SyncEventHandler] Failed to broadcast: {}", e);
            }
        }
    }
}

impl EventHandler<DesktopSyncEvent> for SyncEventHandler {
    fn handle(&self, event: DesktopSyncEvent) {
        // 克隆必要的数据用于异步任务
        let session_manager = self.session_manager.clone();
        let config_manager = self.config_manager.clone();
        let ws_manager = self.ws_manager;

        tokio::spawn(async move {
            let handler = SyncEventHandler {
                session_manager,
                config_manager,
                ws_manager,
            };
            handler.process_event(event).await;
        });
    }
}
