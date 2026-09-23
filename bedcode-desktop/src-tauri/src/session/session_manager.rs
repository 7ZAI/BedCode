//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布
//! 重构后只负责流程编排，各职责已拆分到独立模块

use crate::enums::{ExecutionEnvironment, SessionLaunchConfig, SessionStatus, SessionType};
use crate::events::DesktopSyncEvent;
use crate::pty::PtySession;
use crate::session::session_lifecycle::SessionLifecycleEvent;
use crate::session::{
    input_line::{SessionInputListener, SubmittedLineTracker},
    session_components::{
        CanonicalRendererRegistry, DefaultCanonicalRendererRegistry, DefaultPtyRegistry, DefaultSessionInfoRegistry,
        PtyRegistry, RendererSource, ResizeOutcome, SessionInfoRegistry,
    },
    session_lifecycle::SessionLifecycleListener,
    session_output::{GlobalOutputManager, SessionOutputSink},
};
use crate::session::{SessionInfo, SessionInfoView, SessionStatusEvent};
use crate::system::config::AppConfig;
use crate::system::constants::ENV_BEDCODE_SESSION_ID;
use crate::system::error_boundary::spawn_with_error_boundary;
use crate::Result;
use chrono::Utc;
use portable_pty::CommandBuilder;
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
    /// 会话状态变更广播（前端事件转发与 WS 通道经 [`Self::subscribe_status`] 订阅）
    status_tx: broadcast::Sender<SessionStatusEvent>,
    /// 运行标志
    running: Arc<AtomicBool>,
    /// 同步事件发送器（用于向客户端广播增量数据）
    sync_tx: RwLock<Option<broadcast::Sender<DesktopSyncEvent>>>,
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
    /// 会话属主登记表（审计票 04）：`session-id → 创建方 plugin_id`。
    ///
    /// **内核只存不解释**：属主是一个不透明的身份串，与 pty / ws / mdns / core-task
    /// 各域的句柄 owner 列同形。「先过权限门、再查属主」这条不变量的判定发生在
    /// host_impl（那里有 plugin_id 与错误文案口径），本表只提供事实：
    /// - 写入：`create_session_from_spec`（创建即登记，与 `session_info.insert` 同批）
    /// - 读出：[`Self::session_owner`]（`None` = 内核/宿主自建，无属主 → 插件一律不可操作）
    /// - 清理：`remove_session_with_source`（会话销毁连带注销，不留孤儿键）
    ///
    /// 不进 `SessionInfo` / `SessionInfoView`：属主是宿主侧访问控制事实，
    /// 不是给前端与移动端的展示字段，线协议形状因此零变化。
    session_owners: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
}

// ==================== 业务命令装配（pty 引擎与业务语义的唯一边界） ====================

/// `SessionLaunchConfig` → argv 形态 `CommandBuilder`（业务侧对 pty 引擎的填充）
///
/// 2026-09-23 PTY 解耦票：宿主 `pty/` 引擎不再认识业务配置、不做 shell 包装，
/// 本函数是「业务会话语义 → 引擎原语」的**唯一**翻译点：
///
/// - **argv**：直接取插件算好的 `command_args`（`launch.rs::build_argv` 已完成
///   `bash -lic` / `wsl.exe -d <distro> -- bash -lic` / PowerShell `-Command`
///   三环境的包装与转义；WSL 路径转换与单引号转义同在插件侧）。本层不做任何
///   二次解释——`config.command` 仅是诊断用的人类可读串。
/// - **cwd**：仅 Windows / Linux 显式设置；WSL2 的 cwd 语义由 argv 脚本内的
///   `cd '<wsl 路径>'` 表达（宿主侧工作目录是 Windows 路径，设了也无意义）。
/// - **env**：`config.env_vars` 透传。
/// - **`BEDCODE_SESSION_ID`**：在业务侧注入（Claude Code hooks 靠它关联
///   BedCode 会话）。注意这是**业务身份**，故不进 pty 引擎；插件私有 PTY
///   （host-pty）不注入。
///
/// 缺 argv / argv0 为空即显性报错：旧 shell 包装路径已退役，不存在静默回退。
fn launch_command(session_id: &str, config: &SessionLaunchConfig) -> Result<CommandBuilder> {
    let Some(argv0) = config.command_args.first().filter(|a| !a.trim().is_empty()) else {
        return Err(crate::AppError::Pty(format!(
            "会话启动缺少 argv（commandArgs 为空或 argv[0] 为空，session {}）",
            session_id
        )));
    };
    let mut builder = CommandBuilder::new(argv0);
    for arg in &config.command_args[1..] {
        builder.arg(arg);
    }
    if matches!(
        config.environment,
        ExecutionEnvironment::Windows { .. } | ExecutionEnvironment::Linux
    ) {
        builder.cwd(&config.working_dir);
    }
    for (key, value) in &config.env_vars {
        builder.env(key, value);
    }
    builder.env(ENV_BEDCODE_SESSION_ID, session_id);
    Ok(builder)
}

impl SessionManager {
    /// 获取会话状态变化广播发送器
    pub fn status_tx(&self) -> broadcast::Sender<SessionStatusEvent> {
        self.status_tx.clone()
    }

    /// 创建新的 Session Manager（使用具体实现）
    ///
    /// 无库依赖：v21 起内核不再读会话配置表（命名唯一化 / config→launch 映射 /
    /// 重启执行器全部归插件），故不再注入 `SessionStorage`。
    ///
    /// 无 PTY 工厂依赖（2026-09-23 PTY 解耦票）：命令装配在 [`launch_command`]，
    /// 引擎直接经 `PtySession::with_command` 创建——旧 `PtySessionHandler` 注入点
    /// 随业务 sink / 业务配置类型一并退役。
    pub fn new() -> Self {
        let pty_registry = Arc::new(DefaultPtyRegistry::new());
        let session_info = Arc::new(DefaultSessionInfoRegistry::new());
        let canonical_renderer = Arc::new(DefaultCanonicalRendererRegistry::new());
        let (status_tx, _) = broadcast::channel(AppConfig::global().channels.status_broadcast_capacity);
        let running = Arc::new(AtomicBool::new(true));
        let lifecycle_listeners = Arc::new(RwLock::new(Vec::new()));
        let input_listeners = Arc::new(RwLock::new(Vec::new()));

        Self {
            pty_registry,
            session_info,
            canonical_renderer,
            status_tx,
            running,
            sync_tx: RwLock::new(None),
            lifecycle_listeners,
            input_listeners,
            submitted_line_tracker: SubmittedLineTracker::new(),
            input_log_throttle: std::sync::atomic::AtomicU64::new(0),
            annotations: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            session_owners: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
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

    /// 按启动规格创建会话（票 09 `create-with-spec` 的执行端）
    ///
    /// `launch_config` 由插件算好（命名唯一化 / config→launch 映射 / 尺寸决策在
    /// 插件侧完成），本方法不再读配置表（`storage.get_config`）、不再走命名服务与
    /// 配置映射服务——内核只按 `start` 分支执行：Creating 事件 → PTY 创建 →
    /// 输出注册（早于启动）→ 启动 / 不启动 → 生命周期 → 会话记录 → 事件分发。
    ///
    /// 行为对标（票 09「两条创建路径经插件编排且行为等价」，host-business-decarriage
    /// 收尾后宿主侧只剩本执行端）：
    /// - `start = true`：Running + 正统渲染端归属启动端 + Created 生命周期事件；
    /// - `start = false`：Starting + 不启动进程 + 不注册输出管理器 + 无 Created 事件。
    ///
    /// 顺序不变量（既有全链路测试守护，不得破坏）：
    /// - 输出消费者（GlobalOutputManager）注册 **早于** PTY 启动（PtyReader 随
    ///   start() 即刻读输出，注册晚了首帧会以 "session not found" 丢弃）；
    /// - 启动失败时回滚输出注册（防孤儿会话残留）；
    /// - `source_device` 决定正统渲染端初始归属（启动端固定，防移动端单独启动
    ///   会话时首次 resize 误弹覆盖确认）。
    /// - `owner`（票 04）= 创建方插件 id，登记进属主表供 host_impl 做「先权限后属主」
    ///   判定；`None` = 内核/宿主自建（无属主，插件一律不可操作）。
    pub async fn create_session_from_spec(
        &self,
        launch_config: SessionLaunchConfig,
        config_id: String,
        source_device: Option<String>,
        start: bool,
        session_id: Option<&str>,
        owner: Option<&str>,
    ) -> Result<String> {
        // 分发 Creating 事件（同步阻塞，确保 hooks 在 PTY 启动前就位）
        self.dispatch_lifecycle_event(SessionLifecycleEvent::Creating {
            config_id: config_id.clone(),
            command: launch_config.command.clone(),
            working_dir: launch_config.working_dir.clone(),
            source_device: source_device.clone(),
        })
        .await;

        // 会话 id：调用方指定（重启 = 同一 id 重建）或本次生成
        let session_id = match session_id {
            Some(sid) => sid.to_string(),
            None => uuid::Uuid::new_v4().to_string(),
        };
        // 命令与输出汇都在业务层装配（pty 引擎只收算好的 argv）
        let command = launch_command(&session_id, &launch_config)?;
        let sink = Arc::new(SessionOutputSink::new(&session_id));
        let pty_session = PtySession::with_command(
            session_id.clone(),
            launch_config.name.clone(),
            launch_config.cols,
            launch_config.rows,
            command,
            sink,
        )?;

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
            // 只创建不启动：不注册输出管理器（进程未启动无输出源，与两阶段启动
            // 第一阶段语义一致）；PTY 已 openpty 就绪，由后续
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
        // 属主登记（票 04）：与会话记录同批写入，创建失败路径已在上方 return，
        // 因此不会出现「有记录无属主」或「有属主无记录」的半态
        if let Some(owner) = owner {
            self.session_owners
                .write()
                .await
                .insert(session_id.clone(), owner.to_string());
        }

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
        let status_tx = self.status_tx.clone();
        let pty_registry = self.pty_registry.clone();
        let line_tracker = self.submitted_line_tracker.clone();
        // 票 3：自然退出路径与 kill 同语义——同步事件（SessionStopped）与生命周期
        // Stopped 都分发（任务域「意外退出兜底」依赖生命周期 Stopped，见插件
        // `task/state.rs`；此前 Hold 压制下自然退出永不触发，任务卡死）
        // sync_tx 是 tauri RwLock（不可 clone）——预读 sender 副本进闭包
        let sync_tx_sender = self.sync_tx.read().await.clone();
        let lifecycle_listeners = self.lifecycle_listeners.clone();
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
                        "PTY 终态（业务会话线：翻 Stopped + 分发）"
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
                            session_name: session_name.clone(),
                        });
                    }

                    // 发布同步事件：会话停止（与 kill 路径同形状，移动端会话列表刷新）
                    if let Some(sender) = &sync_tx_sender {
                        let _ = sender.send(DesktopSyncEvent::SessionStopped {
                            session_id: sid.clone(),
                            source_device: None,
                        });
                    }

                    // 分发生命周期 Stopped（异步通知，插件会话生命周期监听收尾）
                    for listener in lifecycle_listeners.read().await.iter().cloned().collect::<Vec<_>>() {
                        listener.on_session_lifecycle(&SessionLifecycleEvent::Stopped {
                            session_id: sid.clone(),
                            source_device: None,
                        });
                    }
                }
            }
        });
    }

    /// 获取会话
    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        self.session_info.get(session_id).await
    }

    /// 会话属主查询（票 04）：返回创建方插件 id；`None` = 无属主（内核/宿主自建）
    ///
    /// 只提供事实不做判断——「先权限门后属主」的判定与错误文案在 host_impl，
    /// 与 pty / ws / mdns 的句柄属主判定同一分层。
    pub async fn session_owner(&self, session_id: &str) -> Option<String> {
        self.session_owners.read().await.get(session_id).cloned()
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

    /// 业务线 PTY 计数（**引擎事实**：在册 **且** 未终结；供关停守卫 / 诊断）
    ///
    /// 判据 = `is_running() && !output_terminated()`，与 `host-pty` 的 `is-running`
    /// 原语同一组合（`running_verdict`）。语义边界与今日关窗守卫口径**逐格对齐**：
    ///
    /// - **含**「已 openpty 但尚未启动」的会话（今日 `Starting`；两阶段第一阶段）
    /// - **含**运行中的会话（今日 `Running`）
    /// - **不含**已 kill / 已自然退出的会话（今日 `Stopped`）
    ///
    /// 两路合取不可省：业务线的 PTY 注册表**不随自然退出摘除**（只有 kill / remove
    /// 才摘），而引擎的 `running` 标志**只有 kill / 销毁才翻下**（业务线依赖该语义），
    /// 故只看任一项都会把已退出的会话算成还在。
    ///
    /// 用途（会话引擎下沉 P1 开放点 4）：关窗守卫判据由「会话状态 ∈ {Running,
    /// Starting}」改为「PTY 计数 > 0」——功能等价，且不引入会话语义与插件调用依赖
    /// （窗口关闭路径不得被插件阻塞）。
    pub async fn live_pty_count(&self) -> usize {
        self.pty_registry
            .list()
            .await
            .iter()
            .filter(|session| session.is_running() && !session.output_terminated())
            .count()
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
        let _ = self.status_tx.send(SessionStatusEvent {
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
        // 属主注销（票 04）：同上不留孤儿键——重启编排会以同一 id 重建并由
        // create-with-spec 重新登记属主
        self.session_owners.write().await.remove(session_id);

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
        self.status_tx.subscribe()
    }

    /// 获取会话状态
    pub async fn get_session_status(&self, session_id: &str) -> Option<SessionStatus> {
        self.session_info.get_status(session_id).await
    }

    /// 更新会话状态
    pub async fn update_session_status(&self, session_id: &str, status: SessionStatus) {
        self.session_info.update_status(session_id, status).await;
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
        Self::new()
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

    /// 票 09 行为契约 A：`create_session_from_spec(start=false)` 产出的会话形状
    /// 逐项符合两阶段启动第一阶段（Starting / 名称用 spec 传入 / config_id 透传 /
    /// started_at 空 / 正统渲染端初始为空），且内核不做二次命名。
    ///
    /// host-business-decarriage 收尾后宿主侧已无 `create_session_no_start` 对照
    /// 基准（编排全在插件），本用例只钉执行端自身契约。
    #[tokio::test]
    async fn test_create_session_from_spec_start_false_is_starting() {
        use crate::enums::ExecutionEnvironment;
        use std::collections::HashMap;

        let manager = SessionManager::default();
        let config_id = "cfg-equiv".to_string();

        // 插件算好的唯一化名 + start=false（不 spawn 进程）
        let launch_config = SessionLaunchConfig {
            name: "cfg-equiv".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            command_args: vec!["bash".to_string()],
            env_vars: HashMap::new(),
            cols: 120,
            rows: 40,
        };
        let spec_sid = manager
            .create_session_from_spec(launch_config, config_id.clone(), None, false, None, None)
            .await
            .expect("spec create");
        let spec_info = manager.session_info.get(&spec_sid).await.expect("spec info");
        assert_eq!(spec_info.status, SessionStatus::Starting, "start=false → Starting");
        assert_eq!(spec_info.config_id, config_id.as_str(), "configId 透传");
        assert_eq!(spec_info.name, "cfg-equiv", "内核不再二次命名");
        assert_eq!(spec_info.started_at, None);
        assert_eq!(manager.canonical_renderer_of(&spec_sid).await, None);

        // 名称由插件决策：同一 spec 名两次创建不互撞（宿主不干预唯一化）
        let mut names: Vec<String> = manager.list_sessions().await.into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(names, vec!["cfg-equiv".to_string()]);

        // 清理：不 spawn 进程，仅释放 openpty 的 slave fd
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
            command_args: vec!["bash".to_string()],
            env_vars: HashMap::new(),
            cols: 100,
            rows: 30,
        };
        let sid = manager
            .create_session_from_spec(launch_config, "cfg-1".to_string(), None, true, None, None)
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
        let manager = SessionManager::default();
        let sid = seed_idle_session(&manager, "cfg-ann").await;

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
        let manager = SessionManager::default();
        let sid = seed_idle_session(&manager, "cfg-ann2").await;

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
        let manager = SessionManager::default();
        let sid = seed_idle_session(&manager, "cfg-ann3").await;
        assert!(manager.annotate_session(&sid, "k", "v").await);
        assert!(!manager.session_annotations(&sid).await.is_empty());

        manager.remove_session(&sid).await.expect("remove session");
        assert!(manager.session_annotations(&sid).await.is_empty(), "会话移除即连带清槽");
    }

    // ==================== 业务命令装配（launch_command，2026-09-23 PTY 解耦票） ====================

    /// argv 原样进 builder（宿主零解释）+ `BEDCODE_SESSION_ID` 按会话 id 注入
    #[test]
    fn launch_command_passes_argv_and_injects_session_id_env() {
        use crate::enums::ExecutionEnvironment;

        let mut env_vars = std::collections::HashMap::new();
        env_vars.insert("FOO".to_string(), "bar".to_string());
        let config = SessionLaunchConfig {
            name: "cfg".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash -lic 'ls'（仅诊断，宿主不解释）".to_string(),
            command_args: vec!["bash".to_string(), "-lic".to_string(), "ls".to_string()],
            env_vars,
            cols: 80,
            rows: 24,
        };

        let cmd = launch_command("sess-42", &config).expect("argv 必须被接受");
        let argv: Vec<String> = cmd.get_argv().iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(argv, ["bash", "-lic", "ls"], "argv 必须原样透传（宿主不做 shell 解释）");
        assert_eq!(
            cmd.get_env("FOO").map(|v| v.to_string_lossy().into_owned()),
            Some("bar".to_string()),
            "env_vars 透传"
        );
        assert_eq!(
            cmd.get_env(ENV_BEDCODE_SESSION_ID).map(|v| v.to_string_lossy().into_owned()),
            Some("sess-42".to_string()),
            "业务会话身份按会话 id 注入（Claude Code hooks 关联用）"
        );
    }

    /// cwd 仅原生环境（Windows / Linux）显式设置；WSL2 的 cwd 语义在 argv 脚本的 cd 内
    #[test]
    fn launch_command_sets_cwd_only_for_native_environments() {
        use crate::enums::ExecutionEnvironment;

        let config = |environment| SessionLaunchConfig {
            name: "cfg".to_string(),
            environment,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            command_args: vec!["bash".to_string()],
            env_vars: std::collections::HashMap::new(),
            cols: 80,
            rows: 24,
        };

        let linux = launch_command("s1", &config(ExecutionEnvironment::Linux)).expect("linux 必须装配成功");
        assert!(linux.get_cwd().is_some(), "Linux 显式设置 cwd");

        let wsl = launch_command(
            "s2",
            &config(ExecutionEnvironment::Wsl2 {
                distro: "Ubuntu".to_string(),
            }),
        )
        .expect("wsl2 必须装配成功");
        assert!(wsl.get_cwd().is_none(), "WSL2 不设宿主 cwd（cwd 在 argv 脚本的 cd 内）");
    }

    /// 缺 argv / argv0 为空 → 显性报错（旧 shell 包装路径已退役，无静默回退）
    #[test]
    fn launch_command_rejects_empty_argv() {
        use crate::enums::ExecutionEnvironment;

        let config = |command_args: Vec<String>| SessionLaunchConfig {
            name: "cfg".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            command_args,
            env_vars: std::collections::HashMap::new(),
            cols: 80,
            rows: 24,
        };

        for argv in [Vec::new(), vec!["   ".to_string()]] {
            let err = launch_command("sess-x", &config(argv.clone())).unwrap_err();
            assert!(
                matches!(err, crate::AppError::Pty(_)),
                "argv 非法必须显性报错，got: {err:?}（argv={argv:?}）"
            );
        }
    }

    fn renderer_desktop() -> RendererSource {
        RendererSource::Desktop
    }

    /// 测试用会话夹具：经执行端建一个「只创建不启动」的会话（不 spawn 进程）。
    ///
    /// host-business-decarriage 收尾后宿主侧不再有读配置表的创建路径，测试直接
    /// 注入插件会算好的 launch spec（命名 / config→launch 映射属插件决策）。
    async fn seed_idle_session(manager: &SessionManager, config_id: &str) -> String {
        use crate::enums::ExecutionEnvironment;
        manager
            .create_session_from_spec(
                SessionLaunchConfig {
                    name: config_id.to_string(),
                    environment: ExecutionEnvironment::Linux,
                    working_dir: "/tmp".to_string(),
                    command: "bash".to_string(),
                    command_args: vec!["bash".to_string()],
                    env_vars: std::collections::HashMap::new(),
                    cols: 120,
                    rows: 40,
                },
                config_id.to_string(),
                None,
                false,
                None,
                None,
            )
            .await
            .expect("seed idle session")
    }

    /// 生命周期监听器记录 mock（与 session_e2e 的 LifecycleCapture 同形）
    #[derive(Default)]
    struct RecordingLifecycleListener {
        events: std::sync::Arc<tokio::sync::RwLock<Vec<SessionLifecycleEvent>>>,
    }

    impl SessionLifecycleListener for RecordingLifecycleListener {
        fn on_session_lifecycle(&self, event: &SessionLifecycleEvent) {
            let events = std::sync::Arc::clone(&self.events);
            let event = event.clone();
            tokio::task::spawn(async move {
                events.write().await.push(event);
            });
        }
    }

    /// 票 3：业务会话**自然退出**（统一 ReleaseOnSpawn）→ 既有终态处理链解锁——
    /// 状态翻 Stopped + SessionStatusEvent + 生命周期 Stopped + SessionStopped 同步
    /// 事件。此前 Hold 压制下自然退出不可观测（状态滞留 Running、任务域「意外退出
    /// 兜底」永不触发而卡死，见插件 task/state.rs 注释）。
    #[tokio::test]
    async fn test_natural_exit_marks_stopped_and_dispatches_lifecycle() {
        use crate::enums::ExecutionEnvironment;
        use std::collections::HashMap;

        let manager = SessionManager::default();
        let listener = std::sync::Arc::new(RecordingLifecycleListener::default());
        manager.register_lifecycle_listener(listener.clone()).await;
        // spawn 前订阅状态事件（避免错过 Stopped）
        let mut status_rx = manager.subscribe_status();

        let sid = manager
            .create_session_from_spec(
                SessionLaunchConfig {
                    name: "natural-exit".to_string(),
                    environment: ExecutionEnvironment::Linux,
                    working_dir: "/tmp".to_string(),
                    command: "exit 7".to_string(),
                    command_args: vec!["bash".to_string(), "-c".to_string(), "exit 7".to_string()],
                    env_vars: HashMap::new(),
                    cols: 100,
                    rows: 30,
                },
                "cfg-natural".to_string(),
                None,
                true,
                None,
                None,
            )
            .await
            .expect("spawn exit 7");

        // 等状态事件：自然退出 → Stopped（不 kill）
        let status = tokio::time::timeout(std::time::Duration::from_secs(5), status_rx.recv())
            .await
            .expect("natural exit status event timeout")
            .expect("status event");
        assert_eq!(
            status.new_status,
            SessionStatus::Stopped,
            "自然退出必须翻 Stopped（不再滞留 Running）"
        );
        assert_eq!(status.session_id, sid);

        // 生命周期 Stopped 必须分发（任务域意外退出兜底依赖）
        let events = listener.events.read().await.clone();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SessionLifecycleEvent::Stopped { session_id, .. } if session_id == &sid)),
            "生命周期 Stopped 必须分发, got: {events:?}"
        );

        // 状态落库
        let info = manager.get_session(&sid).await.expect("session info");
        assert_eq!(info.status, SessionStatus::Stopped, "会话记录状态必须落 Stopped");

        // 引擎事实（开放点 4 的判据前提）：自然退出后**不计入**活 PTY
        //
        // 前提断言不可省：业务线在册句柄不随自然退出摘除（只有 kill / remove 才摘），
        // 故「活 PTY 计数」必须叠加终结位才如实——若有人把判据简化成「注册表条数」或
        // 只看 `is_running()`（引擎的 running 标志只有 kill/销毁才翻下），本用例转红。
        assert!(
            manager.pty_registry.list_ids().await.contains(&sid),
            "前提：自然退出不摘除业务线在册句柄"
        );
        assert_eq!(
            manager.live_pty_count().await,
            0,
            "自然退出后不计入活 PTY（在册 ≠ 活）"
        );

        manager.remove_session(&sid).await.expect("cleanup");
    }

    /// 引擎事实：PTY 计数 = 在册且未终结（与今日关窗守卫口径逐格对齐）
    ///
    /// 阶梯：只建不启（`Starting`）→ 计；再建一个运行中的 → 计两条；kill 一条 → 剩一条；
    /// 摘除 → 清零。
    #[tokio::test]
    async fn live_pty_count_counts_unterminated_ptys() {
        use crate::enums::ExecutionEnvironment;
        use std::collections::HashMap;

        let manager = SessionManager::default();
        let idle = seed_idle_session(&manager, "cfg-idle").await;
        assert_eq!(
            manager.live_pty_count().await,
            1,
            "只建不启也占一条 PTY（今日守卫把 Starting 也算运行中）"
        );

        let running = manager
            .create_session_from_spec(
                SessionLaunchConfig {
                    name: "live".to_string(),
                    environment: ExecutionEnvironment::Linux,
                    working_dir: "/tmp".to_string(),
                    command: "bash".to_string(),
                    command_args: vec!["bash".to_string()],
                    env_vars: HashMap::new(),
                    cols: 100,
                    rows: 30,
                },
                "cfg-live".to_string(),
                None,
                true,
                None,
                None,
            )
            .await
            .expect("spawn live session");
        assert_eq!(manager.live_pty_count().await, 2, "运行中的会话同样计入");

        manager.kill_session(&running).await.expect("kill");
        assert_eq!(manager.live_pty_count().await, 1, "kill 后该条不再计入");

        manager.remove_session(&idle).await.expect("cleanup idle");
        assert_eq!(manager.live_pty_count().await, 0, "摘除后清零");
        manager.remove_session(&running).await.expect("cleanup running");
    }
}
