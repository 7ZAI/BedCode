//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布
//! 重构后只负责流程编排，各职责已拆分到独立模块

use crate::events::DesktopSyncEvent;
use crate::session::{SessionInfo, SessionRestartEvent, SessionStatusEvent};
use crate::session::session_lifecycle::SessionLifecycleEvent;
use crate::pty::{
    PtyOutputEvent, PtySessionHandler, PtyHandler,
    FrontendOutputHandler,
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
    input_line::{SessionInputListener, SubmittedLineTracker},
    session_lifecycle::SessionLifecycleListener,
    session_output::GlobalOutputManager,
    storage::{SessionStorage, SessionStore},
};
use crate::system::error_boundary::spawn_with_error_boundary;
use crate::enums::{SessionStatus, SessionType};
use crate::Result;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{broadcast, RwLock};

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
    /// AppHandle（用于为每个会话启动 FrontendOutputHandler）
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    /// 同步事件发送器（用于向客户端广播增量数据）
    sync_tx: RwLock<Option<broadcast::Sender<DesktopSyncEvent>>>,
    /// 资源目录路径（用于项目级 hooks 脚本复制）
    resource_dir: Arc<PathBuf>,
    /// 会话生命周期监听器注册表
    lifecycle_listeners: Arc<RwLock<Vec<Arc<dyn SessionLifecycleListener>>>>,
    /// 会话输入监听器注册表（提交输入行观察，见 ADR 0001）
    input_listeners: Arc<RwLock<Vec<Arc<dyn SessionInputListener>>>>,
    /// 提交输入行重建器（每会话字节流缓冲区）
    submitted_line_tracker: SubmittedLineTracker,
}

impl SessionManager {
    /// 设置 AppHandle（用于为每个会话启动 FrontendOutputHandler）
    ///
    /// 在启动会话前设置，每个新会话创建后会自动 subscribe_output 并 spawn handler
    pub async fn set_app_handle(&self, app_handle: AppHandle) {
        let mut handle = self.app_handle.write().await;
        *handle = Some(app_handle);
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
        let lifecycle_listeners = Arc::new(RwLock::new(Vec::new()));
        let input_listeners = Arc::new(RwLock::new(Vec::new()));

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
            app_handle: Arc::new(RwLock::new(None)),
            sync_tx: RwLock::new(None),
            resource_dir,
            lifecycle_listeners,
            input_listeners,
            submitted_line_tracker: SubmittedLineTracker::new(),
        }
    }

    /// 设置同步事件发送器
    ///
    /// 在初始化时设置，用于向客户端广播增量数据
    pub async fn set_sync_tx(&self, sync_tx: broadcast::Sender<DesktopSyncEvent>) {
        let mut tx = self.sync_tx.write().await;
        *tx = Some(sync_tx);
    }

    /// 注册会话生命周期监听器
    ///
    /// 监听器在会话关键生命周期节点被调用（Creating/Created/Stopping/Stopped）
    pub async fn register_lifecycle_listener(&self, listener: Arc<dyn SessionLifecycleListener>) {
        let mut listeners = self.lifecycle_listeners.write().await;
        tracing::info!("SessionLifecycleListener registered (total: {})", listeners.len() + 1);
        listeners.push(listener);
    }

    /// 移除指定插件的生命周期监听器
    ///
    /// 插件停用时调用，移除该插件注册的 PluginLifecycleListener
    pub async fn remove_lifecycle_listener(&self, plugin_id: &str) {
        let mut listeners = self.lifecycle_listeners.write().await;
        let before = listeners.len();
        listeners.retain(|l| l.plugin_id() != Some(plugin_id));
        let removed = before - listeners.len();
        if removed > 0 {
            tracing::info!("Removed {} lifecycle listener(s) for plugin '{}'", removed, plugin_id);
        }
    }

    /// 分发会话生命周期事件
    ///
    /// 先克隆监听器快照并释放读锁，再逐个同步调用。
    /// Creating 事件会阻塞直到所有监听器处理完成。
    ///
    /// 不能持锁调用：监听器回调（插件生命周期注册/插件 activate 链路）
    /// 可能反向获取其他锁（如 wasm_plugins），与 activate_plugin 的锁序相反，
    /// 持读锁调用会形成 ABBA 死锁
    async fn dispatch_lifecycle_event(&self, event: SessionLifecycleEvent) {
        let listeners: Vec<Arc<dyn SessionLifecycleListener>> = {
            self.lifecycle_listeners.read().await.iter().cloned().collect()
        };
        for listener in &listeners {
            listener.on_session_lifecycle(&event);
        }
    }

    /// 注册会话输入监听器
    ///
    /// 监听器在用户提交输入行（回车触发）时收到异步通知。
    /// 插件侧注册需 `terminal:observe` 权限（门禁在 host function 层）
    pub async fn register_input_listener(&self, listener: Arc<dyn SessionInputListener>) {
        let mut listeners = self.input_listeners.write().await;
        tracing::info!("SessionInputListener registered (total: {})", listeners.len() + 1);
        listeners.push(listener);
    }

    /// 移除指定插件的输入监听器
    ///
    /// 插件停用时调用，移除该插件注册的 PluginInputListener
    pub async fn remove_input_listener(&self, plugin_id: &str) {
        let mut listeners = self.input_listeners.write().await;
        let before = listeners.len();
        listeners.retain(|l| l.plugin_id() != Some(plugin_id));
        let removed = before - listeners.len();
        if removed > 0 {
            tracing::info!("Removed {} input listener(s) for plugin '{}'", removed, plugin_id);
        }
    }

    /// 异步分发提交输入行事件
    ///
    /// 纯观察语义（见 ADR 0001）：每个监听器独立 spawn 分发，
    /// fire-and-forget、错误隔离（error boundary 兜底 panic），
    /// 不 await 回调、不阻塞输入路径、无顺序保证
    async fn dispatch_input_submitted(&self, session_id: String, text: String) {
        // 快照后立即释放读锁：回调可能反向获取其他锁，持锁分发有 ABBA 死锁风险
        // （与 dispatch_lifecycle_event 同理）
        let listeners: Vec<Arc<dyn SessionInputListener>> = {
            self.input_listeners.read().await.iter().cloned().collect()
        };
        tracing::debug!(
            "dispatch_input_submitted session_id={}, text_len={}, input_listeners={}",
            session_id,
            text.len(),
            listeners.len()
        );
        for listener in listeners {
            let sid = session_id.clone();
            let text = text.clone();
            spawn_with_error_boundary("input_submitted_dispatch", async move {
                listener.on_input_submitted(&sid, &text);
            });
        }

        // 分发到 Rust 静态插件的 TerminalHandler::on_input_submitted（与监听器相同的隔离语义）
        // WASM 插件经各自的 PluginInputListener 接收，两条路径互不重叠
        let plugin_host = crate::system::app_context::AppContext::global().plugin_host();
        spawn_with_error_boundary("input_submitted_terminal_handlers", async move {
            plugin_host.process_input_submitted(&session_id, &text).await;
        });
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
        self.create_session_with_source_and_id(config_id, source_device, None)
            .await
    }

    /// 从配置创建会话（指定会话 ID）
    ///
    /// 供宿主在 wasm 调用上下文之外预生成会话 ID 的异步创建场景使用
    /// （插件定时任务触发，见 host_session_create）：wasm 调用栈内同步创建
    /// 会因生命周期事件（Creating/Created）回灌同一插件实例而死锁，
    /// 因此创建改为宿主异步执行，先返回预生成 ID 供插件记录匹配键。
    pub async fn create_session_with_id(&self, config_id: &str, session_id: &str) -> Result<String> {
        self.create_session_with_source_and_id(config_id, None, Some(session_id))
            .await
    }

    /// 创建会话公共实现：session_id 为 None 时由 PTY 层自行生成
    async fn create_session_with_source_and_id(
        &self,
        config_id: &str,
        source_device: Option<String>,
        session_id: Option<&str>,
    ) -> Result<String> {
        // 从存储加载配置
        let config: crate::db::SessionConfig = self
            .storage
            .get_config(config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        // 分发 Creating 事件（同步阻塞，确保 hooks 在 PTY 启动前就位）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Creating {
            config_id: config_id.to_string(),
            command: config.command.clone(),
            working_dir: config.working_dir.clone(),
            source_device: source_device.clone(),
        }).await;

        // 获取现有会话列表用于生成唯一名称
        let sessions = self.session_info.list().await;
        let session_name = self
            .naming_service
            .generate_unique_name(config_id, &config.name, &sessions);

        // 使用配置映射服务构建启动配置
        let launch_config = self.config_mapper.to_launch_config(&config)?;

        // 创建 PTY 会话（指定 ID 或由 PTY 层生成）
        let pty_session = match session_id {
            Some(sid) => self
                .pty_handler
                .create_session_with_id(sid.to_string(), launch_config.clone())?,
            None => self.pty_handler.create_session(launch_config.clone())?,
        };
        let session_id = pty_session.id().to_string();

        // 订阅 PTY 输出并启动前端转发 task（如果 AppHandle 已设置）
        let app_handle = self.app_handle.read().await.clone();
        if let Some(app_handle) = app_handle {
            let rx = pty_session.subscribe_output().await;
            FrontendOutputHandler::spawn(app_handle, rx);
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

        // 分发 Created 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Created {
            session_id: session_id.clone(),
            config_id: config_id.to_string(),
            name: session_name.clone(),
            working_dir: config.working_dir.clone(),
        }).await;

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

        // 分发 Creating 事件（同步阻塞，确保 hooks 在 PTY 启动前就位）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Creating {
            config_id: config_id.to_string(),
            command: config.command.clone(),
            working_dir: config.working_dir.clone(),
            source_device: None,
        }).await;

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

        // 订阅 PTY 输出并启动前端转发 task（如果 AppHandle 已设置）
        let app_handle = self.app_handle.read().await.clone();
        if let Some(app_handle) = app_handle {
            let rx = pty_session.subscribe_output().await;
            FrontendOutputHandler::spawn(app_handle, rx);
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
        let line_tracker = self.submitted_line_tracker.clone();
        let sid = session_id.to_string();

        tokio::spawn(async move {
            if let Some(session) = pty_registry.get(&sid).await {
                let mut lifecycle_rx = session.subscribe_lifecycle();
                if let Ok(status) = lifecycle_rx.recv().await {
                    let session_status = match status {
                        crate::pty::PtySessionStatus::Error => SessionStatus::Error(None),
                        _ => SessionStatus::Stopped,
                    };

                    // PTY 已退出：清理该会话的输入行缓冲区（残余内容不补发，见 ADR 0001）
                    line_tracker.remove_session(&sid);

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

        // 分发 Creating 事件（同步阻塞，确保 hooks 在 PTY 启动前就位）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Creating {
            config_id: config_id.clone(),
            command: config.command.clone(),
            working_dir: config.working_dir.clone(),
            source_device: None,
        }).await;

        // 构建启动配置（复用配置映射服务）
        let mut launch_config = self.config_mapper.to_launch_config(&config)?;
        launch_config.name = old_name.clone();

        let old_name_for_info = old_name.clone();
        let old_name_for_event = old_name.clone();

        // 创建 PTY 会话（使用相同 ID
        let pty_session = self
            .pty_handler
            .create_session_with_id(session_id.to_string(), launch_config.clone())?;

        // 订阅 PTY 输出并启动前端转发 task（如果 AppHandle 已设置）
        let app_handle = self.app_handle.read().await.clone();
        if let Some(app_handle) = app_handle {
            let rx = pty_session.subscribe_output().await;
            FrontendOutputHandler::spawn(app_handle, rx);
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

        // 分发 Created 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Created {
            session_id: session_id.to_string(),
            config_id: config_id.clone(),
            name: old_name_for_event.clone(),
            working_dir: config.working_dir.clone(),
        }).await;

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

        // 通过插件 TerminalHandler 管道处理输入
        let processed_data = {
            let ctx = crate::system::app_context::AppContext::global();
            let plugin_host = ctx.plugin_host();
            plugin_host.process_terminal_input(session_id, data).await
        };

        // 提交输入行重建 + 异步观察分发（见 ADR 0001）：
        // 观察修改后的最终数据（与 PTY 实际接收一致）；分发为 fire-and-forget，
        // 监听器故障不影响写入，空提交同样通知（宿主不做语义过滤）
        let submitted_lines = self.submitted_line_tracker.feed(session_id, &processed_data);
        tracing::debug!(
            "[SessionManager] write_input line-rebuild session_id={}, data_len={}, submitted_lines={}",
            session_id,
            processed_data.len(),
            submitted_lines.len()
        );
        for line in submitted_lines {
            self.dispatch_input_submitted(session_id.to_string(), line).await;
        }

        self.pty_registry.write_input(session_id, &processed_data).await?;

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

    /// 调整会话终端大小（多个查看者并发时按「最后调整者生效」竞争）
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

        // 分发 Stopping 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Stopping {
            session_id: session_id.to_string(),
            source_device: source_device.clone(),
        }).await;

        // 使用 PTY 注册表终止会话
        if let Err(e) = self.pty_registry.kill(session_id).await {
            tracing::warn!("Failed to kill PTY for session {}: {}", session_id, e);
        }

        // 清理输入行缓冲区（残余内容不补发，见 ADR 0001）
        self.submitted_line_tracker.remove_session(session_id);

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
            source_device: source_device.clone(),
        }).await;

        // 分发 Stopped 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Stopped {
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

        // 清理输入行缓冲区（restart 经此路径重建同 ID 会话，从干净状态开始）
        self.submitted_line_tracker.remove_session(session_id);

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