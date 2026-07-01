//! Server Supervisor
//!
//! 管理服务器子进程的生命周期：启动、停止、重启
//! 通过 IPC (stdin/stdout JSON 行协议) 与子进程通信

use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::shared::system::error::AppError;
use crate::Result;

use super::ipc::IpcCommand;
use super::metrics::ServerMetrics;

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
    child: Option<Child>,
    status: ServerStatus,
    metrics: ServerMetrics,
    metrics_history: VecDeque<TimestampedMetrics>,
    port: u16,
    auto_start: bool,
}

/// 服务器子进程管理器（全局单例）
pub struct ServerSupervisor {
    inner: Arc<RwLock<SupervisorInner>>,
}

impl ServerSupervisor {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<ServerSupervisor> =
            std::sync::LazyLock::new(|| ServerSupervisor {
                inner: Arc::new(RwLock::new(SupervisorInner {
                    child: None,
                    status: ServerStatus::Stopped,
                    metrics: ServerMetrics::default(),
                    metrics_history: VecDeque::with_capacity(60),
                    port: 8765,
                    auto_start: true,
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

    /// 启动服务器子进程
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

        // 获取当前可执行文件路径
        let exe_path = std::env::current_exe()
            .map_err(|e| AppError::Internal(format!("Failed to get current exe path: {}", e)))?;

        let mut child = Command::new(&exe_path)
            .arg("--server-only")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to spawn server process: {}", e)))?;

        // 发送 start 命令到子进程 stdin
        if let Some(mut stdin) = child.stdin.take() {
            let cmd = IpcCommand::Start { port };
            if let Ok(line) = cmd.to_json_line() {
                let _ = stdin.write_all(line.as_bytes());
                let _ = stdin.flush();
            }
            drop(stdin);
        }

        {
            let mut inner = self.inner.write().await;
            inner.child = Some(child);
        }

        // 后台等待子进程启动完成
        let inner_arc = self.inner.clone();
        let port_for_log = port;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            let mut inner = inner_arc.write().await;
            if let Some(ref mut child) = inner.child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::error!("Server child process exited prematurely: {}", status);
                        inner.status = ServerStatus::Stopped;
                        inner.child = None;
                    }
                    Ok(None) => {
                        inner.status = ServerStatus::Running;
                        tracing::info!("Server child process started on port {}", port_for_log);
                    }
                    Err(e) => {
                        tracing::error!("Failed to check server child process: {}", e);
                        inner.status = ServerStatus::Stopped;
                        inner.child = None;
                    }
                }
            }
        });

        // 启动子进程崩溃监控
        self.start_process_monitor();

        Ok(())
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        let mut inner = self.inner.write().await;
        if inner.status != ServerStatus::Running {
            return Err(AppError::WebSocket("Server not running".to_string()));
        }

        // kill 子进程
        if let Some(ref mut child) = inner.child {
            let _ = child.kill();
            let _ = child.wait();
        }

        inner.child = None;
        inner.status = ServerStatus::Stopped;
        inner.metrics = ServerMetrics::default();
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
        let local_ips = crate::shared::system::commands::get_local_ip_addresses();
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

    /// 启动子进程崩溃监控
    fn start_process_monitor(&self) {
        let inner_arc = self.inner.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                let mut inner = inner_arc.write().await;
                if let Some(ref mut child) = inner.child {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            tracing::warn!("Server child process exited: {}", status);
                            inner.status = ServerStatus::Stopped;
                            inner.child = None;
                            break;
                        }
                        Ok(None) => {
                            // 仍在运行
                        }
                        Err(e) => {
                            tracing::error!("Failed to check server process: {}", e);
                            inner.status = ServerStatus::Stopped;
                            inner.child = None;
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        });
    }
}
