//! Server Supervisor
//!
//! 管理服务器生命周期：启动、停止、重启
//! Actix Web 在主进程内运行，通过 WebSocketManager 委托启动
//! 指标采集直接调用 MetricsCollector + sysinfo，无需 IPC
//! 服务器启动时自动启动 mDNS 广播，停止时自动停止

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

use crate::system::error::AppError;
use crate::system::constants::server::{
    DEFAULT_SERVER_PORT, METRICS_HISTORY_CAPACITY, METRICS_SAMPLING_INTERVAL_SECS,
    SERVER_RESTART_DELAY_MS,
};
use crate::system::constants::mdns;
use crate::Result;

use super::metrics::{MetricsCollector, ServerMetrics};

/// crash monitor 轮询间隔（毫秒）
///
/// 正常停止时 supervisor 只置取消信号、不发送广播事件，
/// monitor 需周期性唤醒以感知取消信号并退出，避免任务悬挂
const CRASH_MONITOR_POLL_INTERVAL_MS: u64 = 500;

/// 服务器状态
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running,
}

/// 带时间戳的指标（供前端时序图使用）
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimestampedMetrics {
    pub timestamp_secs: u64,
    pub ws_sent_rate: f64,
    pub ws_recv_rate: f64,
}

/// 服务器状态信息（前端查询用）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerStatusInfo {
    pub status: ServerStatus,
    pub port: u16,
    pub auto_start: bool,
    pub local_ips: Vec<String>,
    pub uptime_secs: Option<u64>,
}

/// Supervisor 内部状态
struct SupervisorInner {
    status: ServerStatus,
    metrics: ServerMetrics,
    metrics_history: VecDeque<TimestampedMetrics>,
    port: u16,
    auto_start: bool,
    /// 服务器启动时间，用于 uptime 计算
    start_time: Option<std::time::Instant>,
    /// sysinfo 采集器，用于 CPU/内存指标
    sys: Arc<std::sync::Mutex<sysinfo::System>>,
    /// 指标采样任务取消标志
    metrics_task_cancel: Arc<AtomicBool>,
}

/// 服务器管理器（全局单例）
///
/// 委托 WebSocketManager 启动/停止 Actix Web，直接采集指标
///
/// # 产品决策：服务器永久自启动
///
/// 桌面端本地功能（终端背景图 `/static/terminal-bg`、插件 HTTP 端点等）依赖本服务，
/// 用户关闭服务会导致这些功能静默失效，因此自启动**永久开启且不再可配置**
/// （见 [`ServerSupervisor::init_config`]；配置文件中遗留的 `network.auto_start`
/// 字段不再生效）。管理页面入口已从 UI 移除，调试者可直访 `/server` 预览。
///
/// 未来规划：可能通过 CLI 开发工具提供服务重启能力（复用现有
/// [`ServerSupervisor::start`] / [`stop`](Self::stop) / [`restart`](Self::restart)）。
pub struct ServerSupervisor {
    inner: Arc<RwLock<SupervisorInner>>,
}

impl ServerSupervisor {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<ServerSupervisor> =
            std::sync::LazyLock::new(|| ServerSupervisor {
                inner: Arc::new(RwLock::new(SupervisorInner {
                    status: ServerStatus::Stopped,
                    metrics: ServerMetrics::default(),
                    metrics_history: VecDeque::with_capacity(METRICS_HISTORY_CAPACITY),
                    port: DEFAULT_SERVER_PORT,
                    auto_start: true,
                    start_time: None,
                    sys: Arc::new(std::sync::Mutex::new(sysinfo::System::new())),
                    metrics_task_cancel: Arc::new(AtomicBool::new(false)),
                })),
            });
        &INSTANCE
    }

    /// 初始化配置
    ///
    /// `auto_start` 参数保留仅为 API 兼容：产品决策为服务器永久自启动，
    /// 无论配置如何，启动阶段一律自动开启（`network.auto_start` 配置项废弃）。
    /// 未来 CLI 开发工具可调用 [`start`](Self::start) 重启服务。
    pub async fn init_config(&self, port: u16, auto_start: bool) {
        let mut inner = self.inner.write().await;
        inner.port = port;
        // 忽略传入值，恒为开启：用户不可关闭服务器（关闭会导致本地功能静默失效）
        inner.auto_start = true;
        let _ = auto_start;
    }

    /// 启动服务器（主进程内 Actix Web）
    pub async fn start(&self, port: u16) -> Result<()> {
        {
            let inner = self.inner.read().await;
            if inner.status == ServerStatus::Running || inner.status == ServerStatus::Starting {
                return Err(AppError::WebSocket("Server already running or starting".to_string()));
            }
        }

        {
            let mut inner = self.inner.write().await;
            inner.status = ServerStatus::Starting;
            inner.port = port;
        }

        // 委托 WebSocketManager 启动 Actix Web
        let ws_manager = crate::server::ws::WebSocketManager::global();
        match ws_manager.start(port).await {
            Ok(_handle) => {
                // 重置指标采集器，确保 uptime 和计数器从零开始
                MetricsCollector::global().reset();

                let mut inner = self.inner.write().await;
                inner.status = ServerStatus::Running;
                inner.start_time = Some(std::time::Instant::now());

                // 取消旧的指标采样任务（防御性：确保 stop() 遗漏时也能停止）
                inner.metrics_task_cancel.store(true, Ordering::Relaxed);

                // 取消标志供采样任务与 crash monitor 共用
                let cancel_flag = Arc::new(AtomicBool::new(false));
                inner.metrics_task_cancel = cancel_flag.clone();

                // 指标采样按配置总开关启动（network.metrics_enabled，默认关闭；
                // 调试者需在 config.properties 开启后重启服务生效）
                if crate::system::config::AppConfig::global().network.metrics_enabled {
                    let inner_arc = self.inner.clone();
                    tokio::spawn(metrics_sampling_task(inner_arc, cancel_flag.clone()));
                }

                // 监听 Actix 线程异常退出事件
                // 当 crash monitor 检测到 Actix 线程崩溃时，会发送 ServerEvent::Stopped。
                // 正常停止由 supervisor 自身处理状态，不广播事件（见 WebSocketManager::stop），
                // 因此 monitor 只需感知取消信号即退出，避免悬挂
                let event_rx = ws_manager.subscribe();
                let inner_for_monitor = self.inner.clone();
                let monitor_cancel = cancel_flag.clone();
                tokio::spawn(async move {
                    let mut rx = event_rx;
                    loop {
                        if monitor_cancel.load(Ordering::Relaxed) {
                            tracing::debug!("Server crash monitor cancelled");
                            break;
                        }
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(CRASH_MONITOR_POLL_INTERVAL_MS),
                            rx.recv(),
                        )
                        .await
                        {
                            // 收到 Started 事件（正常启动广播）：忽略，继续监听崩溃
                            Ok(Ok(crate::server::ws::ServerEvent::Started)) => {}
                            // 收到 Stopped 事件（仅崩溃路径发送）→ 判定为崩溃
                            Ok(Ok(crate::server::ws::ServerEvent::Stopped)) => {
                                let mut inner = inner_for_monitor.write().await;
                                // 仅在 Running 状态下处理（避免与正常 stop 冲突）
                                if inner.status == ServerStatus::Running {
                                    tracing::error!(
                                        "Server crashed unexpectedly, updating supervisor state"
                                    );
                                    inner.status = ServerStatus::Stopped;
                                    inner.start_time = None;
                                    inner.metrics_task_cancel.store(true, Ordering::Relaxed);
                                }
                                break;
                            }
                            // 广播通道已关闭或事件丢失（lag）：终止监听
                            Ok(Err(_)) => break,
                            // 轮询超时：回到循环开头检查取消信号
                            Err(_) => {}
                        }
                    }
                });

                tracing::info!("Server started on port {} (in-process)", port);

                // 服务器启动后自动阻止系统休眠
                crate::system::power::power_manager().enable();

                // 服务器启动后自动启动 mDNS 广播
                start_mdns_advertisement(port);

                Ok(())
            }
            Err(e) => {
                let mut inner = self.inner.write().await;
                inner.status = ServerStatus::Stopped;
                tracing::error!(error = %e, port, "Failed to start server");
                Err(e)
            }
        }
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        {
            let inner = self.inner.read().await;
            if inner.status != ServerStatus::Running && inner.status != ServerStatus::Starting {
                return Err(AppError::WebSocket("Server not running".to_string()));
            }
        }

        // 取消指标采样任务
        {
            let inner = self.inner.read().await;
            inner.metrics_task_cancel.store(true, Ordering::Relaxed);
        }

        // 先置为 Stopped 再执行实际停止：
        // 即便未来有代码路径在停止期间发送 ServerEvent::Stopped，
        // crash monitor 读到非 Running 状态也不会误判为崩溃
        {
            let mut inner = self.inner.write().await;
            inner.status = ServerStatus::Stopped;
        }

        // 委托 WebSocketManager 停止 Actix Web
        let ws_manager = crate::server::ws::WebSocketManager::global();
        if let Err(e) = ws_manager.stop().await {
            // 停止失败：服务器可能仍在运行，恢复 Running 状态供重试
            let mut inner = self.inner.write().await;
            inner.status = ServerStatus::Running;
            return Err(e);
        }

        let mut inner = self.inner.write().await;
        inner.metrics = ServerMetrics::default();
        inner.metrics_history.clear();
        inner.start_time = None;

        // 服务器停止后释放休眠阻止
        crate::system::power::power_manager().disable();

        // 服务器停止时自动停止 mDNS 广播
        stop_mdns_advertisement();

        tracing::info!("Server stopped");
        Ok(())
    }

    /// 重启服务器
    pub async fn restart(&self) -> Result<()> {
        let (port, is_running) = {
            let inner = self.inner.read().await;
            (inner.port, inner.status == ServerStatus::Running)
        };
        if is_running {
            self.stop().await?;
            tokio::time::sleep(std::time::Duration::from_millis(SERVER_RESTART_DELAY_MS)).await;
        }
        self.start(port).await
    }

    /// 获取服务器状态信息
    pub async fn get_status_info(&self) -> ServerStatusInfo {
        let inner = self.inner.read().await;
        let local_ips = crate::commands::system::get_local_ip_addresses();
        let uptime_secs = inner.start_time.map(|t| t.elapsed().as_secs());
        ServerStatusInfo {
            status: inner.status.clone(),
            port: inner.port,
            auto_start: inner.auto_start,
            local_ips,
            uptime_secs,
        }
    }

    /// 获取最新指标
    pub async fn get_metrics(&self) -> ServerMetrics {
        self.inner.read().await.metrics.clone()
    }

    /// 获取指标历史
    pub async fn get_metrics_history(&self) -> Vec<TimestampedMetrics> {
        self.inner.read().await.metrics_history.iter().cloned().collect()
    }

    /// 服务器是否运行中
    pub async fn is_running(&self) -> bool {
        self.inner.read().await.status == ServerStatus::Running
    }

    /// 更新端口配置
    pub async fn update_port(&self, port: u16) -> Result<()> {
        let mut inner = self.inner.write().await;
        inner.port = port;
        Ok(())
    }

    /// 更新自启动配置
    ///
    /// 产品决策：自启动永久开启，本方法为 no-op（保留接口供旧命令调用，
    /// 实际值恒为 true，见 [`init_config`](Self::init_config)）
    pub async fn update_auto_start(&self, _auto_start: bool) {
        let mut inner = self.inner.write().await;
        inner.auto_start = true;
    }
}

/// 采集当前进程的 CPU/内存占用
///
/// sysinfo 首次 refresh_cpu_usage() 只建立基线，需要两次刷新才能得到准确值。
/// 为避免在调用链中 sleep，在采样任务启动时做一次预热刷新，
/// 此处假设预热已完成，直接读取即可。
fn collect_process_metrics(sys: &mut sysinfo::System) -> (f64, u64) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let pid = sysinfo::Pid::from(std::process::id() as usize);
    sys.process(pid)
        .map(|proc| (proc.cpu_usage() as f64, proc.memory()))
        .unwrap_or((0.0, 0))
}

/// 指标采样后台任务（每 5 秒采样一次）
///
/// 首次循环做预热刷新（sysinfo 需要两次 refresh 才能获得准确 CPU 值），
/// 第二次循环开始才有有效数据
async fn metrics_sampling_task(
    inner: Arc<RwLock<SupervisorInner>>,
    cancel: Arc<AtomicBool>,
) {
    // 预热：首次 refresh 建立基线
    {
        let inner_guard = inner.read().await;
        let mut sys = inner_guard.sys.lock().unwrap();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
    }

    loop {
        if cancel.load(Ordering::Relaxed) {
            tracing::debug!("Metrics sampling task cancelled");
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(METRICS_SAMPLING_INTERVAL_SECS)).await;

        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let inner_guard = inner.read().await;
        let sys_arc = inner_guard.sys.clone();
        drop(inner_guard);

        // 异步获取连接数（WsSessionRegistry 是 async 的）
        let connections = crate::server::ws::registry::WsSessionRegistry::global()
            .client_count()
            .await;

        // 同步采集进程 CPU/内存
        let (cpu_percent, memory_bytes) = {
            let mut sys = sys_arc.lock().unwrap();
            collect_process_metrics(&mut sys)
        };

        let collector = MetricsCollector::global();
        let metrics = collector.sample(connections, cpu_percent, memory_bytes);

        let mut inner = inner.write().await;
        let entry = TimestampedMetrics {
            timestamp_secs: metrics.uptime_secs,
            ws_sent_rate: metrics.ws_sent_rate,
            ws_recv_rate: metrics.ws_recv_rate,
        };
        inner.metrics_history.push_back(entry);
        if inner.metrics_history.len() > METRICS_HISTORY_CAPACITY {
            inner.metrics_history.pop_front();
        }
        inner.metrics = metrics;
    }
}

/// 启动 mDNS 广播
fn start_mdns_advertisement(port: u16) {
    // 设备名取自全局 SystemInfo（用户设置的电脑名），与 mDNS 实例名保持一致
    let device_name = crate::system::app_context::AppContext::global()
        .system_info()
        .device_name
        .clone();
    let service_name = format!("{}{}", mdns::SERVICE_NAME_PREFIX, device_name);

    tokio::spawn(async move {
        let ctx = crate::system::app_context::AppContext::global();
        let advertiser = ctx.mdns_advertiser();
        let a = advertiser.read().await;

        let mut txt_records = std::collections::HashMap::new();
        txt_records.insert(mdns::TXT_KEY_PLATFORM.to_string(), mdns::TXT_VALUE_PLATFORM.to_string());
        txt_records.insert(mdns::TXT_KEY_DEVICE_NAME.to_string(), service_name.clone());
        txt_records.insert(mdns::TXT_KEY_VERSION.to_string(), env!("CARGO_PKG_VERSION").to_string());

        let config = crate::mdns::types::AdvertiseConfig {
            service_name,
            port,
            txt_records,
        };

        if let Err(e) = a.start(config).await {
            tracing::error!("[ServerSupervisor] Failed to start mDNS advertisement: {}", e);
        }
    });
}

/// 停止 mDNS 广播
fn stop_mdns_advertisement() {
    tokio::spawn(async move {
        let ctx = crate::system::app_context::AppContext::global();
        let advertiser = ctx.mdns_advertiser();
        let a = advertiser.read().await;

        if let Err(e) = a.stop().await {
            tracing::error!("[ServerSupervisor] Failed to stop mDNS advertisement: {}", e);
        }
    });
}
