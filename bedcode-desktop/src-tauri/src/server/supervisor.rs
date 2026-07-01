//! Server Supervisor
//!
//! 管理服务器生命周期：启动、停止、重启
//! Actix Web 在主进程内运行，通过 WebSocketManager 委托启动
//! 指标采集直接调用 MetricsCollector + sysinfo，无需 IPC

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

use crate::error::AppError;
use crate::Result;

use super::metrics::{MetricsCollector, ServerMetrics};

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
                    metrics_history: VecDeque::with_capacity(60),
                    port: 8765,
                    auto_start: true,
                    start_time: None,
                    sys: Arc::new(std::sync::Mutex::new(sysinfo::System::new())),
                    metrics_task_cancel: Arc::new(AtomicBool::new(false)),
                })),
            });
        &INSTANCE
    }

    /// 初始化配置
    pub async fn init_config(&self, port: u16, auto_start: bool) {
        let mut inner = self.inner.write().await;
        inner.port = port;
        inner.auto_start = auto_start;
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
        let ws_manager = crate::websocket_manager::WebSocketManager::global();
        match ws_manager.start(port).await {
            Ok(_handle) => {
                let mut inner = self.inner.write().await;
                inner.status = ServerStatus::Running;
                inner.start_time = Some(std::time::Instant::now());

                // 启动指标采样任务
                let cancel_flag = Arc::new(AtomicBool::new(false));
                inner.metrics_task_cancel = cancel_flag.clone();
                let inner_arc = self.inner.clone();
                tokio::spawn(metrics_sampling_task(inner_arc, cancel_flag));

                tracing::info!("Server started on port {} (in-process)", port);
                Ok(())
            }
            Err(e) => {
                let mut inner = self.inner.write().await;
                inner.status = ServerStatus::Stopped;
                tracing::error!("Failed to start server: {}", e);
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

        // 委托 WebSocketManager 停止 Actix Web
        let ws_manager = crate::websocket_manager::WebSocketManager::global();
        ws_manager.stop().await?;

        let mut inner = self.inner.write().await;
        inner.status = ServerStatus::Stopped;
        inner.metrics = ServerMetrics::default();
        inner.metrics_history.clear();
        inner.start_time = None;

        tracing::info!("Server stopped");
        Ok(())
    }

    /// 重启服务器
    pub async fn restart(&self) -> Result<()> {
        let port = {
            let inner = self.inner.read().await;
            inner.port
        };
        if self.inner.read().await.status == ServerStatus::Running {
            self.stop().await?;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        self.start(port).await
    }

    /// 获取服务器状态信息
    pub async fn get_status_info(&self) -> ServerStatusInfo {
        let inner = self.inner.read().await;
        let local_ips = crate::commands::system::get_local_ip_addresses();
        ServerStatusInfo {
            status: inner.status.clone(),
            port: inner.port,
            auto_start: inner.auto_start,
            local_ips,
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
    pub async fn update_auto_start(&self, auto_start: bool) {
        let mut inner = self.inner.write().await;
        inner.auto_start = auto_start;
    }
}

/// 采集当前进程的 CPU/内存占用
fn collect_process_metrics(sys: &mut sysinfo::System) -> (f64, u64) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let pid = sysinfo::Pid::from(std::process::id() as usize);
    sys.process(pid)
        .map(|proc| (proc.cpu_usage() as f64, proc.memory()))
        .unwrap_or((0.0, 0))
}

/// 指标采样后台任务（每 5 秒采样一次）
async fn metrics_sampling_task(
    inner: Arc<RwLock<SupervisorInner>>,
    cancel: Arc<AtomicBool>,
) {
    loop {
        if cancel.load(Ordering::Relaxed) {
            tracing::debug!("Metrics sampling task cancelled");
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

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
        if inner.metrics_history.len() > 60 {
            inner.metrics_history.pop_front();
        }
        inner.metrics = metrics;
    }
}
