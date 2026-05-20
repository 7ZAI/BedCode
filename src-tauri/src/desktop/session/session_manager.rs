//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布
//! 重构后只负责流程编排，各职责已拆分到独立模块

use crate::desktop::model::{SessionInfo, SessionRestartEvent, SessionStatusEvent};
use crate::desktop::pty::{PtyOutputEvent, PtySessionHandler, PtyHandler};
use crate::desktop::session::{
    config_mapper::{ConfigMapper, DefaultConfigMapper},
    event_bus::{DefaultSessionEventBus, SessionEventBus},
    naming_service::{DefaultNamingService, NamingService},
    output_cache::{DefaultOutputCache, OutputCache},
    pty_registry::{DefaultPtyRegistry, PtyRegistry},
    session_info::{DefaultSessionInfoRegistry, SessionInfoRegistry},
    status_detector::{DefaultStatusDetector, StatusDetector},
    storage::{SessionStorage, SessionStore},
};
use crate::shared::enums::{SessionStatus, SessionType};
use crate::Result;
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Session Manager
///
/// 重构后只负责协调各服务，不直接操作状态存储
/// 使用具体类型实现，但保持了服务解耦（各服务独立可替换）
pub struct SessionManager {
    /// PTY 会话注册表
    pty_registry: Arc<DefaultPtyRegistry>,
    /// 会话信息注册表
    session_info: Arc<DefaultSessionInfoRegistry>,
    /// 输出缓存
    output_cache: Arc<DefaultOutputCache>,
    /// 事件总线
    event_bus: Arc<DefaultSessionEventBus>,
    /// 命名服务
    naming_service: Arc<DefaultNamingService>,
    /// 配置映射服务
    config_mapper: Arc<DefaultConfigMapper>,
    /// 状态检测服务
    status_detector: Arc<DefaultStatusDetector>,
    /// PTY 处理器
    pty_handler: Arc<PtySessionHandler>,
    /// 会话存储（数据库操作）
    storage: Arc<SessionStorage>,
    /// 运行标志
    running: Arc<AtomicBool>,
}

impl SessionManager {
    /// 获取输出广播发送器
    pub fn output_tx(&self) -> broadcast::Sender<PtyOutputEvent> {
        self.event_bus.output_sender()
    }

    /// 获取会话状态变化广播发送器
    pub fn status_tx(&self) -> broadcast::Sender<SessionStatusEvent> {
        self.event_bus.status_sender()
    }

    /// 获取会话重启广播发送器
    pub fn restart_tx(&self) -> broadcast::Sender<SessionRestartEvent> {
        self.event_bus.restart_sender()
    }

    /// 创建新的 Session Manager（使用具体实现）
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        let pty_handler = Arc::new(PtySessionHandler::new());
        Self::new_with_handlers(storage, pty_handler)
    }

    /// 从数据库创建 Session Manager（兼容旧 API）
    pub fn from_database(db: crate::shared::db::Database) -> Self {
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(SessionStorage::new(db));
        let pty_handler = Arc::new(PtySessionHandler::new());
        Self::new_with_handlers(storage, pty_handler)
    }

    /// 创建新的 Session Manager（使用具体类型注入）
    pub fn new_with_handlers(
        storage: Arc<SessionStorage>,
        pty_handler: Arc<PtySessionHandler>,
    ) -> Self {
        let pty_registry = Arc::new(DefaultPtyRegistry::new());
        let session_info = Arc::new(DefaultSessionInfoRegistry::new());
        let output_cache = Arc::new(DefaultOutputCache::new(1000));
        let event_bus = Arc::new(DefaultSessionEventBus::new());
        let naming_service = Arc::new(DefaultNamingService::new());
        let config_mapper = Arc::new(DefaultConfigMapper::new());
        let status_detector = Arc::new(DefaultStatusDetector::new());
        let running = Arc::new(AtomicBool::new(true));

        Self {
            pty_registry,
            session_info,
            output_cache,
            event_bus,
            naming_service,
            config_mapper,
            status_detector,
            pty_handler,
            storage,
            running,
        }
    }

    /// 从配置创建会话
    pub async fn create_session(&self, config_id: &str) -> Result<String> {
        // 从存储加载配置
        let config = self
            .storage
            .get_config(config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        // 获取现有会话列表用于生成唯一名称
        let sessions = self.session_info.list().await;
        let session_name = self
            .naming_service
            .generate_unique_name(config_id, &config.name, &sessions);

        // 使用配置映射服务构建启动配置
        let launch_config = self.config_mapper.to_launch_config(&config)?;

        // 创建 PTY 会话
        let pty_session = self.pty_handler.create_session(launch_config.clone())?;
        let session_id = pty_session.id().to_string();

        // 启动输出转发器
        self.start_output_forwarder(&session_id, pty_session.subscribe_output()).await;

        // 启动生命周期处理器
        self.start_lifecycle_handler(&session_id).await;

        // 创建会话信息
        let info = SessionInfo {
            id: session_id.clone(),
            config_id: config_id.to_string(),
            name: session_name.clone(),
            status: SessionStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            stopped_at: None,
            session_type: SessionType::Pty,
        };

        // 保存到各服务
        self.pty_registry.insert(session_id.clone(), pty_session).await;
        self.session_info.insert(info).await;

        tracing::info!("Session created: {} ({})", session_name, session_id);
        Ok(session_id)
    }

    /// 启动输出转发器
    async fn start_output_forwarder(&self, session_id: &str, mut rx: broadcast::Receiver<PtyOutputEvent>) {
        let output_tx = self.event_bus.output_sender();
        let running = self.running.clone();
        let output_cache = self.output_cache.clone();
        let sid = session_id.to_string();

        tokio::spawn(async move {
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match rx.recv().await {
                    Ok(event) => {
                        // 缓存输出
                        output_cache.cache(event.clone()).await;
                        // 只在有活跃订阅者时才转发，减少无效消息
                        if output_tx.receiver_count() > 0 {
                            let _ = output_tx.send(event);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Output channel closed for session: {}", sid);
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Output channel lagged {} messages for session: {}", n, sid);
                    }
                }
            }
            tracing::debug!("Output forwarder stopped for session: {}", sid);
        });
    }

    /// 启动生命周期处理器
    async fn start_lifecycle_handler(&self, session_id: &str) {
        let session_info = self.session_info.clone();
        let status_tx = self.event_bus.status_sender();
        let pty_registry = self.pty_registry.clone();
        let sid = session_id.to_string();

        tokio::spawn(async move {
            if let Some(session) = pty_registry.get(&sid).await {
                let mut lifecycle_rx = session.subscribe_lifecycle();
                if let Ok(status) = lifecycle_rx.recv().await {
                    let session_status = match status {
                        crate::desktop::pty::PtySessionStatus::Error => SessionStatus::Error(None),
                        _ => SessionStatus::Stopped,
                    };

                    session_info.update_status_with_time(&sid, session_status.clone()).await;

                    // 获取会话名称
                    let session_name = session_info
                        .get(&sid)
                        .await
                        .map(|i| i.name)
                        .unwrap_or_default();

                    // 发送状态变化事件
                    if status_tx.receiver_count() > 0 {
                        let _ = status_tx.send(SessionStatusEvent {
                            session_id: sid.clone(),
                            old_status: Some(SessionStatus::Running),
                            new_status: session_status,
                            session_name,
                        });
                    }
                }
            }
        });
    }

    /// 重启会话
    pub async fn restart_session(&self, session_id: &str) -> Result<String> {
        // 获取会话信息
        let (config_id, old_name) = {
            let info = self
                .session_info
                .get(session_id)
                .await
                .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;
            (info.config_id.clone(), info.name.clone())
        };

        // 移除旧会话
        self.remove_session(session_id).await?;

        // 获取配置
        let config = self
            .storage
            .get_config(&config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        // 构建启动配置（复用配置映射服务）
        let mut launch_config = self.config_mapper.to_launch_config(&config)?;
        launch_config.name = old_name.clone();

        let old_name_for_info = old_name.clone();
        let old_name_for_event = old_name.clone();

        // 创建 PTY 会话（使用相同 ID���
        let pty_session = self
            .pty_handler
            .create_session_with_id(session_id.to_string(), launch_config.clone())?;

        // 启动输出转发器
        self.start_output_forwarder(session_id, pty_session.subscribe_output()).await;

        // 启动生命周期处理器
        self.start_lifecycle_handler(session_id).await;

        // 启动 PTY
        pty_session.start().await?;

        // 创建会话信息
        let info = SessionInfo {
            id: session_id.to_string(),
            config_id: config_id.clone(),
            name: old_name_for_info,
            status: SessionStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            stopped_at: None,
            session_type: SessionType::Pty,
        };

        // 保存到各服务
        self.pty_registry
            .insert(session_id.to_string(), pty_session)
            .await;
        self.session_info.insert(info).await;

        tracing::info!("Session restarted: {} ({})", old_name_for_event, session_id);

        // 发送重启事件
        let _ = self.event_bus.restart_sender().send(SessionRestartEvent {
            old_session_id: session_id.to_string(),
            new_session_id: session_id.to_string(),
            session_name: old_name,
        });

        Ok(session_id.to_string())
    }

    /// 获取会话
    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        self.session_info.get(session_id).await
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.session_info.list().await
    }

    /// 向会话写入输入
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        tracing::info!(
            "[SessionManager] write_input session_id={}, data_len={}, data={:?}",
            session_id,
            data.len(),
            &data[..data.len().min(50)]
        );

        self.pty_registry.write_input(session_id, data).await?;

        // 更新会话状态为 Running
        self.session_info
            .update_status(session_id, SessionStatus::Running)
            .await;

        tracing::info!("[SessionManager] write_input OK session_id={}", session_id);
        Ok(())
    }

    /// 发送特殊键
    pub async fn send_special_key(&self, session_id: &str, key: &str) -> Result<()> {
        tracing::info!(
            "[SessionManager] send_special_key session_id={}, key={:?}",
            session_id,
            key
        );

        self.pty_registry.send_special_key(session_id, key).await?;

        tracing::info!("[SessionManager] send_special_key OK session_id={}", session_id);
        Ok(())
    }

    /// 调整会话终端大小
    pub async fn resize_session(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        self.pty_registry.resize(session_id, cols, rows).await
    }

    /// 终止会话
    pub async fn kill_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("kill_session called for: {}", session_id);

        // 使用 PTY 注册表终止会话
        if let Err(e) = self.pty_registry.kill(session_id).await {
            tracing::warn!("Failed to kill PTY for session {}: {}", session_id, e);
        }

        // 更新会话状态
        let session_name = self
            .session_info
            .get(session_id)
            .await
            .map(|i| i.name)
            .unwrap_or_default();

        self.session_info
            .update_status_with_time(session_id, SessionStatus::Stopped)
            .await;

        // 发送状态变化事件
        let _ = self.event_bus.status_sender().send(SessionStatusEvent {
            session_id: session_id.to_string(),
            old_status: Some(SessionStatus::Running),
            new_status: SessionStatus::Stopped,
            session_name,
        });

        tracing::info!("Session killed: {}", session_id);
        Ok(())
    }

    /// 删除会话
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("remove_session called for: {}", session_id);

        // 清理缓存
        self.output_cache.clear(session_id).await;

        // 从各注册表移除
        let _ = self.pty_registry.remove(session_id).await;
        let _ = self.session_info.remove(session_id).await;

        tracing::info!("Session removed: {}", session_id);
        Ok(())
    }

    /// 订阅全局输出
    pub fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent> {
        self.event_bus.output_sender().subscribe()
    }

    /// 缓存 PTY 输出（供移动端订阅时获取历史输出）
    pub async fn cache_output(&self, event: &PtyOutputEvent) {
        self.output_cache.cache(event.clone()).await;
    }

    /// 获取缓存的 PTY 输出
    pub async fn get_output_cache(&self, session_id: &str) -> Vec<PtyOutputEvent> {
        self.output_cache.get(session_id).await
    }

    /// 清理指定会话的缓存
    pub async fn clear_output_cache(&self, session_id: &str) {
        self.output_cache.clear(session_id).await;
    }

    /// 清理所有会话的缓存
    pub async fn clear_all_output_cache(&self) {
        self.output_cache.clear_all().await;
    }

    /// 订阅会话状态变化
    pub fn subscribe_status(&self) -> broadcast::Receiver<SessionStatusEvent> {
        self.event_bus.status_sender().subscribe()
    }

    /// 订阅会话重启
    pub fn subscribe_restart(&self) -> broadcast::Receiver<SessionRestartEvent> {
        self.event_bus.restart_sender().subscribe()
    }

    /// 获取会话状态
    pub async fn get_session_status(&self, session_id: &str) -> Option<SessionStatus> {
        self.session_info.get_status(session_id).await
    }

    /// 更新会话状态
    pub async fn update_session_status(&self, session_id: &str, status: SessionStatus) {
        self.session_info.update_status(session_id, status).await;
    }

    /// 检测等待输入状态
    pub async fn detect_waiting_input(&self, session_id: &str, output: &str) -> bool {
        let waiting = self.status_detector.detect_waiting_input(output);

        if waiting {
            self.update_session_status(session_id, SessionStatus::WaitingInput)
                .await;
        }

        waiting
    }

    /// 清理已停止的会话
    pub async fn cleanup_stopped_sessions(&self) {
        let sessions = self.session_info.list().await;
        let stopped_ids: Vec<String> = sessions
            .iter()
            .filter(|info| info.status == SessionStatus::Stopped)
            .map(|info| info.id.clone())
            .collect();

        for id in stopped_ids {
            let _ = self.pty_registry.remove(&id).await;
            let _ = self.session_info.remove(&id).await;
            tracing::debug!("Cleaned up stopped session: {}", id);
        }
    }

    /// 关闭 SessionManager，停止所有会话
    pub async fn shutdown(&self) {
        tracing::info!("SessionManager shutting down...");
        self.running.store(false, Ordering::SeqCst);

        // 终止所有 PTY 会话
        if let Err(e) = self.pty_registry.kill_all().await {
            tracing::error!("Failed to kill all sessions: {}", e);
        }

        // 清理所有缓存
        self.output_cache.clear_all().await;

        tracing::info!("SessionManager shutdown complete");
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        let db = crate::shared::db::Database::new(std::path::Path::new(":memory:"))
            .expect("Failed to create memory database");
        db.init_schema().expect("Failed to init schema");

        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(SessionStorage::new(db));
        let pty_handler = Arc::new(PtySessionHandler::new());

        Self::new_with_handlers(storage, pty_handler)
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_manager_default() {
        let manager: SessionManager = Default::default();
        assert!(manager.list_sessions().await.is_empty());
    }
}