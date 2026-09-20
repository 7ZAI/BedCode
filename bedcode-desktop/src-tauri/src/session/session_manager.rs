//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布
//! 重构后只负责流程编排，各职责已拆分到独立模块

use crate::enums::{SessionLaunchConfig, SessionStatus, SessionType};
use crate::events::DesktopSyncEvent;
use crate::pty::{PtyHandler, PtySessionHandler};
use crate::session::session_lifecycle::SessionLifecycleEvent;
use crate::session::{
    event_bus::{DefaultSessionEventBus, SessionEventBus},
    input_line::{SessionInputListener, SubmittedLineTracker},
    session_components::{
        resolve_initial_size, CanonicalRendererRegistry, ConfigMapper, DefaultCanonicalRendererRegistry,
        DefaultConfigMapper, DefaultNamingService, DefaultPtyRegistry, DefaultSessionInfoRegistry,
        DefaultStatusDetector, NamingService, PtyRegistry, RendererSource, ResizeOutcome, SessionInfoRegistry,
        StatusDetector,
    },
    session_lifecycle::SessionLifecycleListener,
    session_output::GlobalOutputManager,
    storage::{SessionStorage, SessionStore},
};
use crate::session::{SessionInfo, SessionInfoView, SessionRestartEvent, SessionStatusEvent};
use crate::system::error_boundary::spawn_with_error_boundary;
use crate::Result;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
    /// 正统渲染端注册表（每会话 PTY 尺寸归属端，尺寸裁决 + 背压门控权威）
    canonical_renderer: Arc<DefaultCanonicalRendererRegistry>,
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
    /// 同步事件发送器（用于向客户端广播增量数据）
    sync_tx: RwLock<Option<broadcast::Sender<DesktopSyncEvent>>>,
    /// 资源目录路径（用于项目级 hooks 脚本复制）
    #[allow(dead_code)] // 预留字段：公开构造参数,后续用于项目级 hooks 脚本复制
    resource_dir: Arc<PathBuf>,
    /// 会话生命周期监听器注册表
    lifecycle_listeners: Arc<RwLock<Vec<Arc<dyn SessionLifecycleListener>>>>,
    /// 会话输入监听器注册表（提交输入行观察，见 ADR 0001）
    input_listeners: Arc<RwLock<Vec<Arc<dyn SessionInputListener>>>>,
    /// 提交输入行重建器（每会话字节流缓冲区）
    submitted_line_tracker: SubmittedLineTracker,
    /// 高频输入写日志节流计数：抑制 TUI 高频输入（鼠标移动/焦点序列等）刷屏
    input_log_throttle: std::sync::atomic::AtomicU64,
    /// 会话注解槽（票 11，spec D5）：`session-id → key → value` 不透明键值对。
    /// **内核只搬运透传、绝不解释键名**——槽的键名/取值语义归写入方插件
    /// （本域任务的 `taskStatus` 等键就是插件自己的语义）。contract 期（票 12）
    /// 引擎记录的四个任务字段已摘除，对外形状经 [`SessionManager::session_view`]
    /// 从本槽取值（键名 → 字段名的机械映射见 `task_fields_from_slot`）。
    /// 会话移除时连带清理。
    annotations: Arc<tokio::sync::RwLock<std::collections::HashMap<String, std::collections::HashMap<String, String>>>>,
}

impl SessionManager {
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
        let canonical_renderer = Arc::new(DefaultCanonicalRendererRegistry::new());
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
            canonical_renderer,
            event_bus,
            naming_service,
            config_mapper,
            status_detector,
            pty_handler,
            storage,
            running,
            sync_tx: RwLock::new(None),
            resource_dir,
            lifecycle_listeners,
            input_listeners,
            submitted_line_tracker: SubmittedLineTracker::new(),
            input_log_throttle: std::sync::atomic::AtomicU64::new(0),
            annotations: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
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
            tracing::info!(plugin_id = %plugin_id, count = removed, "Removed lifecycle listener(s)");
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
        let listeners: Vec<Arc<dyn SessionLifecycleListener>> =
            { self.lifecycle_listeners.read().await.iter().cloned().collect() };
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
            tracing::info!(plugin_id = %plugin_id, count = removed, "Removed input listener(s)");
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
        let listeners: Vec<Arc<dyn SessionInputListener>> =
            { self.input_listeners.read().await.iter().cloned().collect() };
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
        tracing::info!(session_id = %session_id, "Registered session in GlobalOutputManager");
    }

    /// 从配置创建会话
    pub async fn create_session(&self, config_id: &str) -> Result<String> {
        self.create_session_with_source(config_id, None, None).await
    }

    /// 从配置创建会话（带来源设备）
    ///
    /// source_device: 触发操作的设备名称，桌面本地操作为 None
    /// initial_size: 启动端终端组件的默认网格（cols, rows），None 时用配置默认值
    pub async fn create_session_with_source(
        &self,
        config_id: &str,
        source_device: Option<String>,
        initial_size: Option<(u16, u16)>,
    ) -> Result<String> {
        self.create_session_with_source_and_id(config_id, source_device, None, initial_size)
            .await
    }

    /// 从配置创建会话（指定会话 ID）
    ///
    /// 供宿主在 wasm 调用上下文之外预生成会话 ID 的异步创建场景使用
    /// （插件定时任务触发，见 host_session_create）：wasm 调用栈内同步创建
    /// 会因生命周期事件（Creating/Created）回灌同一插件实例而死锁，
    /// 因此创建改为宿主异步执行，先返回预生成 ID 供插件记录匹配键。
    pub async fn create_session_with_id(&self, config_id: &str, session_id: &str) -> Result<String> {
        self.create_session_with_source_and_id(config_id, None, Some(session_id), None)
            .await
    }

    /// 创建会话公共实现：session_id 为 None 时由 PTY 层自行生成
    ///
    /// instrument（链路追踪）：会话创建是低频链路关键入口，span 在运行时日志
    /// (runtime/error.*.log) 中以 `create_session_with_source_and_id{...}:` 前缀
    /// 聚合下游事件，便于按会话排查；fmt 层事件 scope 自动输出 span 链
    #[tracing::instrument(skip(self), fields(config_id = %config_id))]
    async fn create_session_with_source_and_id(
        &self,
        config_id: &str,
        source_device: Option<String>,
        session_id: Option<&str>,
        initial_size: Option<(u16, u16)>,
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
        })
        .await;

        // 获取现有会话列表用于生成唯一名称
        let sessions = self.session_info.list().await;
        let session_name = self
            .naming_service
            .generate_unique_name(config_id, &config.name, &sessions);

        // 使用配置映射服务构建启动配置（启动端默认网格覆盖配置缺省值，
        // PTY openpty 即以正确行列创建，避免 80x24 首帧回绕）
        let mut launch_config = self.config_mapper.to_launch_config(&config)?;
        let (cols, rows) = resolve_initial_size(launch_config.cols, launch_config.rows, initial_size);
        launch_config.cols = cols;
        launch_config.rows = rows;

        // 创建 PTY 会话（指定 ID 或由 PTY 层生成）
        let pty_session = match session_id {
            Some(sid) => self
                .pty_handler
                .create_session_with_id(sid.to_string(), launch_config.clone())?,
            None => self.pty_handler.create_session(launch_config.clone())?,
        };
        let session_id = pty_session.id().to_string();

        // 注册输出管理器须在 PTY 启动（PtyReader 随 start() 即刻读 PTY 输出）之前：
        // 注册晚于启动时，首帧输出经 GlobalOutputManager::on_output 以 "session not
        // found" 丢弃——早期字节不进任何队列（移动端订阅/HTTP 历史同源），永久丢失。
        // start 失败时回滚注册，防孤儿会话残留（无 PTY、无订阅者，后续无法注销）
        self.register_output_manager(&session_id).await;
        if let Err(e) = pty_session.start().await {
            GlobalOutputManager::global().unregister_session(&session_id).await;
            return Err(e);
        }

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
        // 正统渲染端初始归属 = 启动端：PTY 在「启动按钮」按下时即创建并运行，
        // 先于任何终端视图打开；其初始网格尺寸由首个打开终端的 resize 确立。
        // 故归属在启动时按来源固定：桌面本地启动（source_device=None）为
        // Desktop；移动端经 HTTP/WS 启动（source_device=claims 设备名）为
        // Mobile{device_name}。归属随启动端确立，避免移动端单独启动会话时
        // 首次 resize 误弹覆盖确认（见 resize_session 裁决）。
        let initial_canonical = match &source_device {
            Some(name) => RendererSource::Mobile {
                device_name: name.clone(),
            },
            None => RendererSource::Desktop,
        };
        self.canonical_renderer.set(&session_id, initial_canonical).await;
        self.session_info.insert(info).await;

        // 分发 Created 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Created {
            session_id: session_id.clone(),
            config_id: config_id.to_string(),
            name: session_name.clone(),
            working_dir: config.working_dir.clone(),
        })
        .await;

        // 发布同步事件：会话创建
        self.publish_sync_event(DesktopSyncEvent::SessionCreated {
            session_id: session_id.clone(),
            source_device,
        })
        .await;

        tracing::info!(session_id = %session_id, "Session created: {}", session_name);
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
        })
        .await;

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
        };

        // 保存到各服务
        self.pty_registry.insert(session_id.clone(), pty_session).await;
        // 正统渲染端初始为空：首位设置尺寸的端经 resize_session 抢占归属
        self.session_info.insert(info).await;

        // 发布同步事件：会话创建（状态为 starting）
        self.publish_sync_event(DesktopSyncEvent::SessionCreated {
            session_id: session_id.clone(),
            source_device: None,
        })
        .await;

        tracing::info!(session_id = %session_id, "Session created (not started): {}", session_name);
        Ok(session_id)
    }

    /// 按启动规格创建会话（票 09 `create-with-spec` 的执行端）
    ///
    /// `launch_config` 由插件算好（命名唯一化 / config→launch 映射 / 尺寸决策在
    /// 插件侧完成），本方法不再读配置表（`storage.get_config`）、不再走命名服务与
    /// 配置映射服务——内核只按 `start` 分支执行：Creating 事件 → PTY 创建 →
    /// 输出注册（早于启动）→ 启动 / 不启动 → 生命周期 → 会话记录 → 事件分发。
    ///
    /// 行为等价对标（票 09「两条创建路径经插件编排且行为等价」）：
    /// - `start = true`：顺序与状态与 [`Self::create_session_with_source_and_id`]
    ///   一致（Running + 正统渲染端归属启动端 + Created 生命周期事件）；
    /// - `start = false`：顺序与状态与 [`Self::create_session_no_start`] 一致
    ///   （Starting + 不启动进程 + 不注册输出管理器 + 无 Created 事件）。
    ///
    /// 顺序不变量（既有全链路测试守护，不得破坏）：
    /// - 输出消费者（GlobalOutputManager）注册 **早于** PTY 启动（PtyReader 随
    ///   start() 即刻读输出，注册晚了首帧会以 "session not found" 丢弃）；
    /// - 启动失败时回滚输出注册（防孤儿会话残留）；
    /// - `source_device` 决定正统渲染端初始归属（启动端固定，防移动端单独启动
    ///   会话时首次 resize 误弹覆盖确认）。
    pub async fn create_session_from_spec(
        &self,
        launch_config: SessionLaunchConfig,
        config_id: String,
        source_device: Option<String>,
        start: bool,
        session_id: Option<&str>,
    ) -> Result<String> {
        // 分发 Creating 事件（同步阻塞，确保 hooks 在 PTY 启动前就位）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Creating {
            config_id: config_id.clone(),
            command: launch_config.command.clone(),
            working_dir: launch_config.working_dir.clone(),
            source_device: source_device.clone(),
        })
        .await;

        // 创建 PTY 会话（指定 ID 或由 PTY 层生成）
        let pty_session = match session_id {
            Some(sid) => self
                .pty_handler
                .create_session_with_id(sid.to_string(), launch_config.clone())?,
            None => self.pty_handler.create_session(launch_config.clone())?,
        };
        let session_id = pty_session.id().to_string();

        if start {
            // 注册输出管理器须在 PTY 启动（PtyReader 随 start() 即刻读 PTY 输出）
            // 之前：注册晚于启动时首帧输出被 GlobalOutputManager::on_output 以
            // "session not found" 丢弃，永久丢失。start 失败时回滚注册，防孤儿
            // 会话残留（无 PTY、无订阅者，后续无法注销）。
            self.register_output_manager(&session_id).await;
            if let Err(e) = pty_session.start().await {
                GlobalOutputManager::global().unregister_session(&session_id).await;
                return Err(e);
            }
        } else {
            // 只创建不启动：不注册输出管理器（进程未启动无输出源，与既有
            // create_session_no_start 分支一致）；PTY 已 openpty 就绪，由后续
            // start_existing_session 接管启动与输出注册。
        }

        // 启动生命周期处理器
        self.start_lifecycle_handler(&session_id).await;

        // 创建会话信息（status 随 start 分支，与既有两条创建路径逐字段一致）
        let info = SessionInfo {
            id: session_id.clone(),
            config_id: config_id.clone(),
            name: launch_config.name.clone(),
            status: if start {
                SessionStatus::Running
            } else {
                SessionStatus::Starting
            },
            created_at: Utc::now(),
            started_at: if start { Some(Utc::now()) } else { None },
            stopped_at: None,
            session_type: SessionType::Pty,
        };

        // 保存到各服务
        self.pty_registry.insert(session_id.clone(), pty_session).await;
        if start {
            // 正统渲染端初始归属 = 启动端（与 create_session_with_source_and_id
            // 同一规则）：桌面本地启动（source_device=None）为 Desktop；移动端
            // 经 HTTP/WS 启动（source_device=claims 设备名）为 Mobile{device_name}。
            let initial_canonical = match &source_device {
                Some(name) => RendererSource::Mobile {
                    device_name: name.clone(),
                },
                None => RendererSource::Desktop,
            };
            self.canonical_renderer.set(&session_id, initial_canonical).await;
        }
        self.session_info.insert(info).await;

        if start {
            // 分发 Created 事件（异步通知；与 create_session_with_source_and_id 一致）
            self.dispatch_lifecycle_event(SessionLifecycleEvent::Created {
                session_id: session_id.clone(),
                config_id: config_id.clone(),
                name: launch_config.name.clone(),
                working_dir: launch_config.working_dir.clone(),
            })
            .await;
        }

        // 发布同步事件：会话创建（start=false 时与 create_session_no_start 同形状）
        self.publish_sync_event(DesktopSyncEvent::SessionCreated {
            session_id: session_id.clone(),
            source_device,
        })
        .await;

        tracing::info!(session_id = %session_id, config_id = %config_id, start, "Session created from spec: {}", launch_config.name);
        Ok(session_id)
    }

    /// 启动已存在的会话（用于延迟启动场景）
    ///
    /// initial_size: 启动端终端组件当前/默认网格。两阶段启动时 PTY 对已按
    /// 配置默认尺寸 openpty，这里在 spawn 前先 resize 到请求端真实尺寸，
    /// 子进程从正确的行列起步（避免 80x24 起步的首帧回绕）。
    pub async fn start_existing_session(&self, session_id: &str, initial_size: Option<(u16, u16)>) -> Result<()> {
        // 获取会话信息
        let session_info = self
            .session_info
            .get(session_id)
            .await
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;

        // 获取 PTY 会话
        let pty_session = self
            .pty_registry
            .get(session_id)
            .await
            .ok_or_else(|| crate::AppError::NotFound(format!("PTY session not found: {}", session_id)))?;

        // 注册到全局输出管理器（启用移动端订阅功能）
        // 必须在启动 PTY 之前注册，否则输出事件会被丢弃
        self.register_output_manager(session_id).await;

        // spawn 前按请求端尺寸调整 PTY（openpty 已完成，resize 仅改内核窗口大小）
        if let Some((cols, rows)) = initial_size.filter(|(c, r)| *c > 0 && *r > 0) {
            if let Err(e) = pty_session.resize(cols, rows).await {
                tracing::warn!(error = %e, session_id = %session_id, cols, rows, "Failed to apply initial size before PTY start");
            }
        }

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
        })
        .await;

        tracing::info!(session_id = %session_id, "Session started: {}", session_name);
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
                if let Ok(terminated) = lifecycle_rx.recv().await {
                    let session_status = match terminated.status {
                        crate::pty::PtySessionStatus::Error => SessionStatus::Error(None),
                        _ => SessionStatus::Stopped,
                    };
                    tracing::debug!(
                        session_id = %sid,
                        exit_code = ?terminated.exit_code,
                        killed = terminated.killed,
                        "PTY 终态（业务会话线仅取状态）"
                    );

                    // PTY 已退出：清理该会话的输入行缓冲区（残余内容不补发，见 ADR 0001）
                    line_tracker.remove_session(&sid);

                    session_info.update_status_with_time(&sid, session_status.clone()).await;

                    // 获取会话名称
                    let session_name = session_info.get(&sid).await.map(|i| i.name).unwrap_or_default();

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
        })
        .await;

        // 构建启动配置（复用配置映射服务）
        let mut launch_config = self.config_mapper.to_launch_config(&config)?;
        launch_config.name = old_name.clone();

        let old_name_for_info = old_name.clone();
        let old_name_for_event = old_name.clone();

        // 创建 PTY 会话（使用相同 ID
        let pty_session = self
            .pty_handler
            .create_session_with_id(session_id.to_string(), launch_config.clone())?;

        // 启动生命周期处理器
        self.start_lifecycle_handler(session_id).await;

        // 注册输出管理器须在 PTY 启动（PtyReader 随 start() 即刻读 PTY 输出）之前：
        // remove_session 已注销本会话，此处必须重新注册，否则 PTY 输出经
        // GlobalOutputManager::on_output 以 "session not found" 丢弃，订阅返回
        // SESSION_NOT_FOUND（前端终端空白）。start 失败时回滚注册，防孤儿会话残留。
        self.register_output_manager(session_id).await;
        if let Err(e) = pty_session.start().await {
            GlobalOutputManager::global().unregister_session(session_id).await;
            return Err(e);
        }

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
        self.pty_registry.insert(session_id.to_string(), pty_session).await;
        // 正统渲染端归属：重启由桌面端发起（source_device=None）→ Desktop
        self.canonical_renderer.set(&session_id, RendererSource::Desktop).await;
        self.session_info.insert(info).await;

        tracing::info!(session_id = %session_id, "Session restarted: {}", old_name_for_event);

        // 分发 Created 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Created {
            session_id: session_id.to_string(),
            config_id: config_id.clone(),
            name: old_name_for_event.clone(),
            working_dir: config.working_dir.clone(),
        })
        .await;

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

    /// 会话注解槽写入（票 11）：`session-id → key → value` 不透明键值对。
    ///
    /// **只搬运透传、绝不解释键名**（spec D5）：键名/取值语义归写入方插件，
    /// 本内核不校验、不解读、不持久化（expland 期纯内存，contract 期再定存储）。
    ///
    /// - 会话存在 → 落槽并返回 `true`（重复写同键 = 覆盖，后写赢）
    /// - 会话不存在 → 不写孤儿键，返回 `false`（调用方显性报错）
    pub async fn annotate_session(&self, session_id: &str, key: &str, value: &str) -> bool {
        let exists = self.session_info.get(session_id).await.is_some();
        if !exists {
            return false;
        }
        let mut slot = self.annotations.write().await;
        slot.entry(session_id.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
        true
    }

    /// 读会话注解槽全量（票 11；不存在或空槽 → 空 map）。用于宿主原语透传回执
    /// （`list-sessions` / `get` 的 `annotations` 字段）、票 12 的对外视图构造与测试断言。
    pub async fn session_annotations(&self, session_id: &str) -> std::collections::HashMap<String, String> {
        self.annotations
            .read()
            .await
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// 会话对外视图（票 12 contract）：引擎记录 + 注解槽任务字段
    ///
    /// 这是**任务语义字段的唯一取值点**——内核记录里已无这四个字段（spec D5），
    /// 对外形状（前端命令 / 控制帧 / 移动端 DTO）经 [`SessionInfoView`] 逐字段保持不变，
    /// 值来自写入方插件落在槽里的键（键值语义归插件，内核机械转发）。
    pub async fn session_view(&self, session_id: &str) -> Option<SessionInfoView> {
        let info = self.session_info.get(session_id).await?;
        let annotations = self.session_annotations(session_id).await;
        Some(SessionInfoView::from_session(info, &annotations))
    }

    /// 全部会话的对外视图（与 [`Self::list_sessions`] 同序：注册表迭代序）
    pub async fn session_views(&self) -> Vec<SessionInfoView> {
        let infos = self.session_info.list().await;
        let mut views = Vec::with_capacity(infos.len());
        for info in infos {
            let annotations = self.session_annotations(&info.id).await;
            views.push(SessionInfoView::from_session(info, &annotations));
        }
        views
    }

    /// 获取会话信息，未找到时返回错误
    pub async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo> {
        self.session_info
            .get(session_id)
            .await
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.session_info.list().await
    }

    /// 向会话写入输入
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        // 高频输入限流日志：TUI 应用（opencode 等）开启鼠标 1003 / 焦点 1004 上报后，
        // 鼠标移动/焦点切换会以每秒数十条输入帧灌入，逐条引日志会刷屏（历史：
        // 每次输入 3 条 INFO-[SessionManager] write_input）。改为节流采样：
        // 前 3 次 + 每 256 次采样一条，保留输入链路可查性
        if self.input_log_throttle.fetch_add(1, Ordering::SeqCst) < 3 {
            // 使用 chars() 确保 UTF-8 安全截断，避免在多字节字符中间切割
            let preview: String = data.chars().take(50).collect();
            tracing::debug!(
                "[SessionManager] write_input session_id={}, data_len={}, data={:?}",
                session_id,
                data.len(),
                preview
            );
        }

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
        if !submitted_lines.is_empty() {
            // 仅提交行有意义时才打日志（逐键输入 submitted_lines 恒为 0，跳过避免刷屏）
            tracing::debug!(
                "[SessionManager] write_input line-rebuild session_id={}, data_len={}, submitted_lines={}",
                session_id,
                processed_data.len(),
                submitted_lines.len()
            );
        }
        for line in submitted_lines {
            self.dispatch_input_submitted(session_id.to_string(), line).await;
        }

        self.pty_registry.write_input(session_id, &processed_data).await?;

        // 更新会话状态为 Running
        self.session_info
            .update_status(session_id, SessionStatus::Running)
            .await;

        Ok(())
    }

    /// 发送特殊键
    pub async fn send_special_key(&self, session_id: &str, key: &str) -> Result<()> {
        // 与 write_input 同一节流（按住退格/回车连发时避免逐次刷屏）
        if self.input_log_throttle.fetch_add(1, Ordering::SeqCst) < 3 {
            tracing::debug!(
                "[SessionManager] send_special_key session_id={}, key={:?}",
                session_id,
                key
            );
        }

        self.pty_registry.send_special_key(session_id, key).await?;

        Ok(())
    }

    /// 调整会话终端大小（多端并发时的正统渲染端裁决）
    ///
    /// 参数：
    /// - `source` 请求方身份（桌面端恒为 Desktop，移动端为 Mobile{device_name}）
    /// - `force` 是否强制覆盖（客户端弹窗确认后置位）
    ///
    /// 规则：无归属时首次请求方即位正统；归属 = 请求方直接应用；归属 ≠ 请求方
    /// 且未 force → 返回 NeedsConfirmation（不应用底层 resize），由请求方弹窗
    /// 确认后带 force 重发；force → 应用并移交归属。
    pub async fn resize_session(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
        source: RendererSource,
        force: bool,
    ) -> Result<ResizeOutcome> {
        let current = self.canonical_renderer.get(session_id).await;
        match &current {
            // 无归属（首次设置者即位正统）或归属 = 请求方：直接应用
            None => {}
            Some(c) if c == &source => {}
            // 归属 = 其他端且未确认覆盖：不应用，返回需确认信号
            Some(_) if !force => {
                tracing::debug!(
                    session_id,
                    source = ?source,
                    current = ?current,
                    "resize blocked: needs confirmation from current canonical renderer"
                );
                return Ok(ResizeOutcome::NeedsConfirmation {
                    current_canonical: current.expect("checked above"),
                });
            }
            // force：覆盖其他端归属
            Some(_) => {}
        }

        self.pty_registry.resize(session_id, cols, rows).await?;
        self.canonical_renderer.set(session_id, source.clone()).await;

        Ok(ResizeOutcome::Applied { canonical: source })
    }

    /// 查询会话当前正统渲染端（背压门控等只读路径）
    pub async fn canonical_renderer_of(&self, session_id: &str) -> Option<RendererSource> {
        self.canonical_renderer.get(session_id).await
    }

    /// 改名（票 10 会话动作）：只改会话记录的展示名，不动 config_id / 状态 / 归属。
    ///
    /// 返回改名前的名字（调用方回执）；未知会话显性 `NotFound`（不静默新建记录）。
    /// 名字合法性（非空）由调用侧原语先仲裁；此处不做唯一化——唯一化策略属插件侧
    /// 会话创建编排（票 09 `generate_unique_name`），改名是用户显式意图，不替用户改写。
    ///
    /// 不派发同步事件（线协议形状保持不变）：改名结果经会话列表拉取可见。
    pub async fn rename_session(&self, session_id: &str, name: &str) -> Result<String> {
        self.session_info
            .rename(session_id, name)
            .await
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))
    }

    /// 终止会话
    pub async fn kill_session(&self, session_id: &str) -> Result<()> {
        self.kill_session_with_source(session_id, None).await
    }

    /// 终止会话（带来源设备）
    ///
    /// source_device: 触发操作的设备名称，桌面本地操作为 None
    pub async fn kill_session_with_source(&self, session_id: &str, source_device: Option<String>) -> Result<()> {
        tracing::info!(session_id = %session_id, "kill_session called");

        // 分发 Stopping 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Stopping {
            session_id: session_id.to_string(),
            source_device: source_device.clone(),
        })
        .await;

        // 使用 PTY 注册表终止会话
        if let Err(e) = self.pty_registry.kill(session_id).await {
            tracing::warn!(session_id = %session_id, error = %e, "Failed to kill PTY for session");
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
        })
        .await;

        // 分发 Stopped 事件（异步通知）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Stopped {
            session_id: session_id.to_string(),
            source_device,
        })
        .await;

        tracing::info!(session_id = %session_id, "Session killed");
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
        tracing::info!(session_id = %session_id, "remove_session called");

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
        // 正统渲染端归属随会话销毁清除
        self.canonical_renderer.clear(session_id).await;

        // 清理输入行缓冲区（restart 经此路径重建同 ID 会话，从干净状态开始）
        self.submitted_line_tracker.remove_session(session_id);

        // 清理会话注解槽（票 11）：会话销毁即连带移除其注解，不残留孤儿键
        self.annotations.write().await.remove(session_id);

        // 发布同步事件：会话删除
        self.publish_sync_event(DesktopSyncEvent::SessionRemoved {
            session_id: session_id.to_string(),
            source_device,
        })
        .await;

        tracing::info!(session_id = %session_id, "Session removed: {}", session_name);
        Ok(())
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
            self.canonical_renderer.clear(&id).await;
            tracing::debug!(session_id = %id, "Cleaned up stopped session");
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

        // 清理会话注解槽（票 11：纯内存态，停机即清空）
        self.annotations.write().await.clear();

        tracing::info!("SessionManager shutdown complete");
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).expect("Failed to create memory database");
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
    use crate::session::RendererSource;

    #[tokio::test]
    async fn test_session_manager_default() {
        let manager: SessionManager = Default::default();
        assert!(manager.list_sessions().await.is_empty());
    }

    /// 正统渲染端裁决：归属 = 其他端且未 force → NeedsConfirmation（不碰底层）
    #[tokio::test]
    async fn test_resize_needs_confirmation_from_other_renderer() {
        let manager = SessionManager::default();
        let current = RendererSource::Mobile {
            device_name: "Pixel-9".to_string(),
        };
        let requester = RendererSource::Mobile {
            device_name: "Redmi-K70".to_string(),
        };
        // 预置归属：当前正统为 Pixel-9
        manager.canonical_renderer.set("s1", current.clone()).await;

        // 他端未 force：返回 NeedsConfirmation，且不调用底层 resize（无会话也不报 NotFound）
        let outcome = manager
            .resize_session("s1", 100, 40, requester.clone(), false)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            ResizeOutcome::NeedsConfirmation {
                current_canonical: current.clone()
            }
        );
        // 归属未被移动端请求方抢占
        assert_eq!(manager.canonical_renderer_of("s1").await, Some(current.clone()));

        // force：尝试应用（无真实会话 → NotFound，证明已越过裁决进入底层调用）
        let err = manager
            .resize_session("s1", 100, 40, requester.clone(), true)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::AppError::NotFound(_)));
        // 归属：force 路径先 resize 后 set —— 底层失败则归属不変
        assert_eq!(manager.canonical_renderer_of("s1").await, Some(current));
    }

    /// 正统渲染端裁决：请求方就是正统端 → 直接应用（无会话 → NotFound 证明已到底层）
    #[tokio::test]
    async fn test_resize_self_is_canonical_applies_directly() {
        let manager = SessionManager::default();
        let desktop = RendererSource::Desktop;
        manager.canonical_renderer.set("s2", renderer_desktop()).await;

        let err = manager.resize_session("s2", 120, 30, desktop, false).await.unwrap_err();
        assert!(matches!(err, crate::AppError::NotFound(_)));
    }

    /// 正统渲染端裁决：无归属时首次请求方即位正统并尝试应用
    #[tokio::test]
    async fn test_resize_first_requester_claims_no_confirmation() {
        let manager = SessionManager::default();
        assert_eq!(manager.canonical_renderer_of("s3").await, None);

        // 无归属：不返回 NeedsConfirmation，直接到底层（无会话 → NotFound）
        let err = manager
            .resize_session(
                "s3",
                80,
                24,
                RendererSource::Mobile {
                    device_name: "Reno-11".to_string(),
                },
                false,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, crate::AppError::NotFound(_)));
        // 归属已确立为首次请求方（先 set 后底层报错？—— 见实现：set 在 resize 之后，
        // 底层失败则不 set；此处只验证回归路径不误判为 NeedsConfirmation）
    }

    /// 会话不存在时的裁决：归属查询为 None → 走应用路径 → NotFound
    #[tokio::test]
    async fn test_resize_unknown_session_falls_through() {
        let manager = SessionManager::default();
        let err = manager
            .resize_session("ghost", 80, 24, RendererSource::Desktop, false)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::AppError::NotFound(_)));
    }

    /// 票 09 行为等价对照 A：`create_session_from_spec(start=false)` 与既有
    /// `create_session_no_start` 产出的会话形状逐项一致（Starting / 名称用
    /// spec 传入 / config_id 透传 / started_at 空 / 正统渲染端初始为空）。
    ///
    /// 对照基准（旧路径）：同一条 linux 配置 → no_start 创建，唯一名首见 = 原名。
    /// spec 路径取名由插件负责（此处直接注入同一「唯一化后」名），两条路径
    /// 并列时名称一致，证明内核不加第二套命名。
    #[tokio::test]
    async fn test_create_session_from_spec_matches_no_start_legacy() {
        use crate::enums::ExecutionEnvironment;
        use std::collections::HashMap;

        let db = crate::db::Database::new(std::path::Path::new(":memory:")).expect("mem db");
        db.init_schema().expect("schema");
        let config = crate::db::SessionConfig::new(
            "cfg-equiv".to_string(),
            "linux".to_string(),
            "/tmp".to_string(),
            "bash".to_string(),
        );
        let config_id = config.id.clone();
        db.create_session_config(&config).expect("create config");
        let manager = SessionManager::from_database(db, Arc::new(std::path::PathBuf::from(".")));

        // 对照基准：旧路径 create_session_no_start（不 spawn 进程）
        let legacy_sid = manager
            .create_session_no_start(&config_id)
            .await
            .expect("legacy no-start");
        let legacy_info = manager.session_info.get(&legacy_sid).await.expect("legacy info");
        assert_eq!(legacy_info.status, SessionStatus::Starting);
        assert_eq!(legacy_info.config_id, config_id.as_str());
        assert_eq!(legacy_info.name, "cfg-equiv", "无竞争时唯一名 = 原名");
        assert_eq!(legacy_info.started_at, None);
        assert_eq!(
            manager.canonical_renderer_of(&legacy_sid).await,
            None,
            "no_start 初始无正统端"
        );

        // spec 路径：同一配置语义 + 插件算好的唯一化名 + start=false
        let launch_config = SessionLaunchConfig {
            name: "cfg-equiv".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            env_vars: HashMap::new(),
            cols: 120,
            rows: 40,
        };
        let spec_sid = manager
            .create_session_from_spec(launch_config, config_id.clone(), None, false, None)
            .await
            .expect("spec create");
        let spec_info = manager.session_info.get(&spec_sid).await.expect("spec info");
        assert_eq!(
            spec_info.status,
            SessionStatus::Starting,
            "start=false → Starting，与 no_start 一致"
        );
        assert_eq!(spec_info.config_id, config_id.as_str(), "configId 透传");
        assert_eq!(spec_info.name, "cfg-equiv", "内核不再二次命名");
        assert_eq!(spec_info.started_at, None);
        assert_eq!(manager.canonical_renderer_of(&spec_sid).await, None);

        // 两会话并列：名称由插件决策，同一 spec 名两次创建不互撞（宿主不干预）
        let mut names: Vec<String> = manager.list_sessions().await.into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(names, vec!["cfg-equiv".to_string(), "cfg-equiv".to_string()]);

        // 清理：不 spawn 进程，仅释放 openpty 的 slave fd
        manager.remove_session(&legacy_sid).await.expect("remove legacy");
        manager.remove_session(&spec_sid).await.expect("remove spec");
    }

    /// 票 09 行为等价对照 B：`create_session_from_spec(start=true)` 与既有
    /// `create_session_with_source_and_id` 的状态语义一致（Running / started_at /
    /// 正统渲染端归属起点 Desktop）。真实 spawn bash，测试后 kill 清理。
    #[tokio::test]
    async fn test_create_session_from_spec_start_true_running_and_desktop_canonical() {
        use crate::enums::ExecutionEnvironment;
        use std::collections::HashMap;

        let manager = SessionManager::default();
        let launch_config = SessionLaunchConfig {
            name: "spawned".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            env_vars: HashMap::new(),
            cols: 100,
            rows: 30,
        };
        let sid = manager
            .create_session_from_spec(launch_config, "cfg-1".to_string(), None, true, None)
            .await
            .expect("spec create + start");
        let info = manager.session_info.get(&sid).await.expect("info");
        assert_eq!(info.status, SessionStatus::Running, "start=true → Running");
        assert!(info.started_at.is_some(), "started_at 记录启动时刻");
        assert_eq!(info.name, "spawned");
        assert_eq!(
            manager.canonical_renderer_of(&sid).await,
            Some(RendererSource::Desktop),
            "桌面本地启动（source=None）→ 正统端 Desktop"
        );

        // 清理：kill 真进程并移除会话
        manager.kill_session(&sid).await.expect("kill spawned bash");
        manager.remove_session(&sid).await.expect("remove spawned");
    }

    /// 票 11 注解槽：写入 → 原样读回（不透明透传，键名/取值含怪字符不解释）；
    /// 同键覆盖后写赢、异键并存互不干扰；会话不存在不写孤儿键
    #[tokio::test]
    async fn test_annotate_roundtrip_opaque_and_overwrite() {
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).expect("mem db");
        db.init_schema().expect("schema");
        let config = crate::db::SessionConfig::new(
            "cfg-ann".to_string(),
            "linux".to_string(),
            "/tmp".to_string(),
            "bash".to_string(),
        );
        let config_id = config.id.clone();
        db.create_session_config(&config).expect("create config");
        let manager = SessionManager::from_database(db, Arc::new(std::path::PathBuf::from(".")));
        let sid = manager
            .create_session_no_start(&config_id)
            .await
            .expect("create session");

        // 不透明透传：怪键名 / 怪取值原样落槽
        assert!(manager.annotate_session(&sid, "taskStatus", "in_progress").await);
        assert!(manager.annotate_session(&sid, "task-reason", "AI 会话 ").await);
        let ann = manager.session_annotations(&sid).await;
        assert_eq!(ann.get("taskStatus").map(String::as_str), Some("in_progress"));
        assert_eq!(ann.get("task-reason").map(String::as_str), Some("AI 会话 "));

        // 同键覆盖、异键并存
        assert!(manager.annotate_session(&sid, "taskStatus", "completed").await);
        let ann = manager.session_annotations(&sid).await;
        assert_eq!(ann.len(), 2, "同键覆盖不新增条目，异键并存");
        assert_eq!(ann.get("taskStatus").map(String::as_str), Some("completed"));
        assert_eq!(ann.get("task-reason").map(String::as_str), Some("AI 会话 "));

        // 未知会话：不写孤儿键
        assert!(!manager.annotate_session("ghost", "taskStatus", "x").await);
        assert!(manager.session_annotations("ghost").await.is_empty());

        manager.remove_session(&sid).await.expect("remove");
    }

    /// 票 12 contract：引擎记录已无任务字段，对外视图的任务字段**只**来自注解槽
    /// —— 写槽前后视图字段的出现/缺省是同一构造点的两个态（含 M2 降级口径）
    #[tokio::test]
    async fn test_annotate_feeds_public_view_task_fields() {
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).expect("mem db");
        db.init_schema().expect("schema");
        let config = crate::db::SessionConfig::new(
            "cfg-ann2".to_string(),
            "linux".to_string(),
            "/tmp".to_string(),
            "bash".to_string(),
        );
        let config_id = config.id.clone();
        db.create_session_config(&config).expect("create config");
        let manager = SessionManager::from_database(db, Arc::new(std::path::PathBuf::from(".")));
        let sid = manager
            .create_session_no_start(&config_id)
            .await
            .expect("create session");

        // 写槽前：视图任务字段缺省（插件未激活 / 未写槽 → 字段为空，M2 降级口径）
        let before = manager.session_view(&sid).await.expect("view");
        assert!(before.task_status.is_none() && before.task_reason.is_none());
        let json = serde_json::to_value(&before).expect("serialize");
        assert!(json.get("taskStatus").is_none() && json.get("taskReason").is_none());

        // 写槽：视图字段随槽取值（引擎记录里没有可被「双写」的第二份数据通道）
        assert!(manager.annotate_session(&sid, "taskStatus", "asking").await);
        assert!(manager.annotate_session(&sid, "taskReason", "等待答复").await);
        let after = manager.session_view(&sid).await.expect("view after");
        assert_eq!(after.task_status.as_deref(), Some("asking"));
        assert_eq!(after.task_reason.as_deref(), Some("等待答复"));
        assert_eq!(after.info.id, sid, "记录字段不受影响");

        // 列表视图与单视图同源同值
        let listed = manager.session_views().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].task_status.as_deref(), Some("asking"));

        manager.remove_session(&sid).await.expect("remove");
    }

    /// 票 11：会话移除连带清理注解槽（不残留孤儿键；restart = 移除 + 同 ID 重建
    /// 经此路径，重建后槽为空态起始）
    #[tokio::test]
    async fn test_annotate_cleaned_on_session_remove() {
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).expect("mem db");
        db.init_schema().expect("schema");
        let config = crate::db::SessionConfig::new(
            "cfg-ann3".to_string(),
            "linux".to_string(),
            "/tmp".to_string(),
            "bash".to_string(),
        );
        let config_id = config.id.clone();
        db.create_session_config(&config).expect("create config");
        let manager = SessionManager::from_database(db, Arc::new(std::path::PathBuf::from(".")));
        let sid = manager
            .create_session_no_start(&config_id)
            .await
            .expect("create session");
        assert!(manager.annotate_session(&sid, "k", "v").await);
        assert!(!manager.session_annotations(&sid).await.is_empty());

        manager.remove_session(&sid).await.expect("remove session");
        assert!(manager.session_annotations(&sid).await.is_empty(), "会话移除即连带清槽");
    }

    fn renderer_desktop() -> RendererSource {
        RendererSource::Desktop
    }
}
