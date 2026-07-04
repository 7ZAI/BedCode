//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布
//! 重构后只负责流程编排，各职责已拆分到独立模块

use crate::events::DesktopSyncEvent;
use crate::session::{SessionInfo, SessionRestartEvent, SessionStatusEvent};
use crate::pty::{
    AsyncPtyOutputListener, PtyOutputEvent, PtySessionHandler, PtyHandler,
};
use crate::session::{
    session_components::{
        ConfigMapper, DefaultConfigMapper,
        DefaultNamingService, NamingService,
        DefaultPtyRegistry, PtyRegistry,
        DefaultSessionInfoRegistry, SessionInfoRegistry,
        DefaultStatusDetector, StatusDetector,
    },
    event_bus::{DefaultSessionEventBus, SessionEventBus},
    session_output::GlobalOutputManager,
    storage::{SessionStorage, SessionStore},
};
use crate::pty::PtyOutputListener;
use crate::enums::{SessionStatus, SessionType};
use crate::system::config::AppConfig;
use crate::Result;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

/// Session Manager
///
/// 重构后只负责协调各服务，不直接操作状态存储
/// 使用具体类型实现，但保持了服务解耦（各服务独立可替换）
pub struct SessionManager {
    /// PTY 会话注册表
    pty_registry: Arc<DefaultPtyRegistry>,
    /// 会话信息注册表
    session_info: Arc<DefaultSessionInfoRegistry>,
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
    /// PTY 输出事件监听器（可动态添加）
    output_listener: Arc<RwLock<Option<Arc<dyn PtyOutputListener>>>>,
    /// 同步事件发送器（用于向客户端广播增量数据）
    sync_tx: RwLock<Option<broadcast::Sender<DesktopSyncEvent>>>,
    /// 资源目录路径（用于项目级 hooks 脚本复制）
    resource_dir: Arc<PathBuf>,
}

impl SessionManager {
    /// 设置 PTY 输出事件监听器
    ///
    /// 在启动会话前设置，用于接收 PTY 输出事件
    /// 传入 Arc<dyn PtyOutputListener>，任何实现该 trait 的类型都可以
    pub async fn set_output_listener(&self, listener: Arc<dyn PtyOutputListener>) {
        let mut output_listener = self.output_listener.write().await;
        *output_listener = Some(listener);
    }

    /// 清除 PTY 输出事件监听器
    pub async fn clear_output_listener(&self) {
        let mut output_listener = self.output_listener.write().await;
        *output_listener = None;
    }

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
    pub fn new(storage: Arc<SessionStorage>, resource_dir: Arc<PathBuf>) -> Self {
        let pty_handler = Arc::new(PtySessionHandler::new());
        Self::new_with_handlers(storage, pty_handler, resource_dir)
    }

    /// 从数据库创建 Session Manager（兼容旧 API）
    pub fn from_database(db: crate::db::Database, resource_dir: Arc<PathBuf>) -> Self {
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(SessionStorage::new(db));
        let pty_handler = Arc::new(PtySessionHandler::new());
        Self::new_with_handlers(storage, pty_handler, resource_dir)
    }

    /// 创建新的 Session Manager（使用具体类型注入）
    pub fn new_with_handlers(
        storage: Arc<SessionStorage>,
        pty_handler: Arc<PtySessionHandler>,
        resource_dir: Arc<PathBuf>,
    ) -> Self {
        let pty_registry = Arc::new(DefaultPtyRegistry::new());
        let session_info = Arc::new(DefaultSessionInfoRegistry::new());
        let event_bus = Arc::new(DefaultSessionEventBus::new());
        let naming_service = Arc::new(DefaultNamingService::new());
        let config_mapper = Arc::new(DefaultConfigMapper::new());
        let status_detector = Arc::new(DefaultStatusDetector::new());
        let running = Arc::new(AtomicBool::new(true));

        Self {
            pty_registry,
            session_info,
            event_bus,
            naming_service,
            config_mapper,
            status_detector,
            pty_handler,
            storage,
            running,
            output_listener: Arc::new(RwLock::new(None)),
            sync_tx: RwLock::new(None),
            resource_dir,
        }
    }

    /// 设置同步事件发送器
    ///
    /// 在初始化时设置，用于向客户端广播增量数据
    pub async fn set_sync_tx(&self, sync_tx: broadcast::Sender<DesktopSyncEvent>) {
        let mut tx = self.sync_tx.write().await;
        *tx = Some(sync_tx);
    }

    /// 发布同步事件
    ///
    /// 内部方法，用于发布 DesktopSyncEvent 到事件总线
    async fn publish_sync_event(&self, event: DesktopSyncEvent) {
        let tx = self.sync_tx.read().await;
        if let Some(sender) = &*tx {
            let _ = sender.send(event);
        }
    }

    /// 为会话注册输出管理器
    /// 在创建 PTY session 后调用，启用移动端订阅功能
    pub async fn register_output_manager(&self, session_id: &str) {
        // 注册会话到全局输出管理器
        let global_manager = GlobalOutputManager::global();
        global_manager.register_session(session_id).await;
        tracing::info!("Registered session {} in GlobalOutputManager", session_id);
    }

    /// 从配置创建会话
    pub async fn create_session(&self, config_id: &str) -> Result<String> {
        self.create_session_with_source(config_id, None).await
    }

    /// 从配置创建会话（带来源设备）
    ///
    /// source_device: 触发操作的设备名称，桌面本地操作为 None
    pub async fn create_session_with_source(&self, config_id: &str, source_device: Option<String>) -> Result<String> {
        // 从存储加载配置
        let config: crate::db::SessionConfig = self
            .storage
            .get_config(config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        // TODO: 项目级 Hooks 自动配置暂时禁用，后续重新设计后启用
        // if config.command.to_lowercase().contains("claude") {
        //     let app_config = AppConfig::global();
        //     let result = crate::plugin::setup::ensure_project_hooks(
        //         &config.working_dir,
        //         app_config.network.port,
        //         &app_config.plugin.token,
        //         &self.resource_dir,
        //     ).await;
        //     if !result.skipped {
        //         tracing::info!("Project hooks setup: {} (skipped={})", result.message, result.skipped);
        //     }
        // }

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

        // 先注册输出监听器（如果已设置），再启动 PTY
        let listener = self.output_listener.read().await.clone();
        if let Some(listener) = listener {
            pty_session.add_output_listener(listener);
        }

        // 启动 PTY 会话
        pty_session.start().await?;

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
            task_status: None,
            task_reason: None,
            task_updated_at: None,
            task_questions: None,
        };

        // 保存到各服务
        self.pty_registry.insert(session_id.clone(), pty_session).await;
        self.session_info.insert(info).await;

        // 注册到全局输出管理器（启用移动端订阅功能）
        self.register_output_manager(&session_id).await;

        // 发布同步事件：会话创建
        self.publish_sync_event(DesktopSyncEvent::SessionCreated {
            session_id: session_id.clone(),
            source_device,
        }).await;

        tracing::info!("Session created: {} ({})", session_name, session_id);
        Ok(session_id)
    }

    /// 创建会话但不启动 PTY（仅创建会话信息）
    /// 返回 session_id，前端准备好后可调用 start_existing_session 启动
    pub async fn create_session_no_start(&self, config_id: &str) -> Result<String> {
        // 从存储加载配置
        let config: crate::db::SessionConfig = self
            .storage
            .get_config(config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        // TODO: 项目级 Hooks 自动配置暂时禁用，后续重新设计后启用
        // if config.command.to_lowercase().contains("claude") {
        //     let app_config = AppConfig::global();
        //     let result = crate::plugin::setup::ensure_project_hooks(
        //         &config.working_dir,
        //         app_config.network.port,
        //         &app_config.plugin.token,
        //         &self.resource_dir,
        //     ).await;
        //     if !result.skipped {
        //         tracing::info!("Project hooks setup: {} (skipped={})", result.message, result.skipped);
        //     }
        // }

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

        // 注册输出监听器
        let listener = self.output_listener.read().await.clone();
        if let Some(listener) = listener {
            pty_session.add_output_listener(listener);
        }

        // 不启动 PTY，只保存会话信息
        // pty_session.start().await?; // 这里不启动

        // 启动生命周期处理器
        self.start_lifecycle_handler(&session_id).await;

        // 创建会话信息（状态为 starting）
        let info = SessionInfo {
            id: session_id.clone(),
            config_id: config_id.to_string(),
            name: session_name.clone(),
            status: SessionStatus::Starting,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            session_type: SessionType::Pty,
            task_status: None,
            task_reason: None,
            task_updated_at: None,
            task_questions: None,
        };

        // 保存到各服务
        self.pty_registry.insert(session_id.clone(), pty_session).await;
        self.session_info.insert(info).await;

        // 发布同步事件：会话创建（状态为 starting）
        self.publish_sync_event(DesktopSyncEvent::SessionCreated {
            session_id: session_id.clone(),
            source_device: None,
        }).await;

        tracing::info!("Session created (not started): {} ({})", session_name, session_id);
        Ok(session_id)
    }

    /// 启动已存在的会话（用于延迟启动场景）
    pub async fn start_existing_session(&self, session_id: &str) -> Result<()> {
        // 获取会话信息
        let session_info = self
            .session_info
            .get(session_id)
            .await
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;

        // 获取 PTY 会话
        let pty_session = self.pty_registry.get(session_id).await
            .ok_or_else(|| crate::AppError::NotFound(format!("PTY session not found: {}", session_id)))?;

        // 注册到全局输出管理器（启用移动端订阅功能）
        // 必须在启动 PTY 之前注册，否则输出事件会被丢弃
        self.register_output_manager(session_id).await;

        // 启动 PTY
        pty_session.start().await?;

        // 更新会话状态为 Running
        let session_name = session_info.name.clone();
        let old_status = session_info.status.clone();
        let mut updated_info = session_info;
        updated_info.status = SessionStatus::Running;
        updated_info.started_at = Some(Utc::now());
        self.session_info.insert(updated_info).await;

        // 发布同步事件：会话状态变化（通知移动端）
        self.publish_sync_event(DesktopSyncEvent::SessionStatusChanged {
            session_id: session_id.to_string(),
            old_status,
            new_status: SessionStatus::Running,
        }).await;

        tracing::info!("Session started: {} ({})", session_name, session_id);
        Ok(())
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
                        crate::pty::PtySessionStatus::Error => SessionStatus::Error(None),
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

        // TODO: 项目级 Hooks 自动配置暂时禁用，后续重新设计后启用
        // if config.command.to_lowercase().contains("claude") {
        //     let app_config = AppConfig::global();
        //     let result = crate::plugin::setup::ensure_project_hooks(
        //         &config.working_dir,
        //         app_config.network.port,
        //         &app_config.plugin.token,
        //         &self.resource_dir,
        //     ).await;
        //     if !result.skipped {
        //         tracing::info!("Project hooks setup: {} (skipped={})", result.message, result.skipped);
        //     }
        // }

        // 构建启动配置（复用配置映射服务）
        let mut launch_config = self.config_mapper.to_launch_config(&config)?;
        launch_config.name = old_name.clone();

        let old_name_for_info = old_name.clone();
        let old_name_for_event = old_name.clone();

        // 创建 PTY 会话（使用相同 ID
        let pty_session = self
            .pty_handler
            .create_session_with_id(session_id.to_string(), launch_config.clone())?;

        // 先注册输出监听器（如果已设置），再启动 PTY
        let listener = self.output_listener.read().await.clone();
        if let Some(listener) = listener {
            pty_session.add_output_listener(listener);
        }

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
            task_status: None,
            task_reason: None,
            task_updated_at: None,
            task_questions: None,
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

    /// 获取会话信息，未找到时返回错误
    pub async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo> {
        self.session_info.get(session_id).await
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.session_info.list().await
    }

    /// 向会话写入输入
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        // 使用 chars() 确保 UTF-8 安全截断，避免在多字节字符中间切割
        let preview: String = data.chars().take(50).collect();
        tracing::info!(
            "[SessionManager] write_input session_id={}, data_len={}, data={:?}",
            session_id,
            data.len(),
            preview
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
        self.kill_session_with_source(session_id, None).await
    }

    /// 终止会话（带来源设备）
    ///
    /// source_device: 触发操作的设备名称，桌面本地操作为 None
    pub async fn kill_session_with_source(&self, session_id: &str, source_device: Option<String>) -> Result<()> {
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

        // 发布同步事件：会话停止
        self.publish_sync_event(DesktopSyncEvent::SessionStopped {
            session_id: session_id.to_string(),
            source_device,
        }).await;

        tracing::info!("Session killed: {}", session_id);
        Ok(())
    }

    /// 删除会话
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        self.remove_session_with_source(session_id, None).await
    }

    /// 删除会话（带来源设备）
    ///
    /// source_device: 触发操作的设备名称，桌面本地操作为 None
    pub async fn remove_session_with_source(&self, session_id: &str, source_device: Option<String>) -> Result<()> {
        tracing::info!("remove_session called for: {}", session_id);

        // 从全局输出管理器注销
        let global_manager = GlobalOutputManager::global();
        global_manager.unregister_session(session_id).await;

        // 在移除前获取会话名称（用于同步通知）
        let session_name = self
            .session_info
            .get(session_id)
            .await
            .map(|i| i.name)
            .unwrap_or_default();

        // 从各注册表移除（PTY 的缓存会随 PTY 一起被清理）
        let _ = self.pty_registry.remove(session_id).await;
        let _ = self.session_info.remove(session_id).await;

        // 发布同步事件：会话删除
        self.publish_sync_event(DesktopSyncEvent::SessionRemoved {
            session_id: session_id.to_string(),
            source_device,
        }).await;

        tracing::info!("Session removed: {} ({})", session_id, session_name);
        Ok(())
    }

    /// 订阅全局输出
    pub fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent> {
        self.event_bus.output_sender().subscribe()
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

        // 终止所有 PTY 会话（缓存会随 PTY 一起清理）
        if let Err(e) = self.pty_registry.kill_all().await {
            tracing::error!("Failed to kill all sessions: {}", e);
        }

        tracing::info!("SessionManager shutdown complete");
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        let db = crate::db::Database::new(std::path::Path::new(":memory:"))
            .expect("Failed to create memory database");
        db.init_schema().expect("Failed to init schema");

        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(SessionStorage::new(db));
        let pty_handler = Arc::new(PtySessionHandler::new());
        let resource_dir = Arc::new(std::path::PathBuf::from("."));

        Self::new_with_handlers(storage, pty_handler, resource_dir)
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