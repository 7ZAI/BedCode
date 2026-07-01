//! Server Supervisor
//!
//! 管理服务器子进程的生命周期：启动、停止、重启
//! 通过 IPC (stdin/stdout JSON 行协议) 与子进程通信

use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::AppError;
use crate::Result;

use super::ipc::{IpcCommand, IpcResponse};
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
    /// 子进程 stdin，保留用于发送 IPC 命令
    child_stdin: Option<ChildStdin>,
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
                    child_stdin: None,
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

        // 取出 stdin/stdout 句柄
        let child_stdin = child.stdin.take();
        let child_stdout = child.stdout.take();

        {
            let mut inner = self.inner.write().await;
            inner.child = Some(child);
            inner.child_stdin = child_stdin;
        }

        // 发送 start 命令到子进程 stdin
        self.send_ipc_command(&IpcCommand::Start { port }).await?;

        // 启动 IPC 读取循环（后台线程读取子进程 stdout）
        if let Some(stdout) = child_stdout {
            self.start_ipc_reader(stdout);
        }

        // 启动子进程崩溃监控
        self.start_process_monitor();

        Ok(())
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        // 尝试通过 IPC 发送优雅停机命令
        let _ = self.send_ipc_command(&IpcCommand::Stop).await;

        // 给子进程一点时间优雅退出
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let mut inner = self.inner.write().await;
        if inner.status != ServerStatus::Running && inner.status != ServerStatus::Starting {
            return Err(AppError::WebSocket("Server not running".to_string()));
        }

        // 强制 kill 子进程（如果仍在运行）
        if let Some(ref mut child) = inner.child {
            match child.try_wait() {
                Ok(Some(_)) => {}
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

        inner.child = None;
        inner.child_stdin = None;
        inner.status = ServerStatus::Stopped;
        inner.metrics = ServerMetrics::default();
        inner.metrics_history.clear();
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

    /// 发送 IPC 命令到子进程 stdin
    async fn send_ipc_command(&self, cmd: &IpcCommand) -> Result<()> {
        let mut inner = self.inner.write().await;
        if let Some(ref mut stdin) = inner.child_stdin {
            if let Ok(line) = cmd.to_json_line() {
                stdin.write_all(line.as_bytes())
                    .map_err(|e| AppError::Internal(format!("Failed to write IPC command: {}", e)))?;
                stdin.flush()
                    .map_err(|e| AppError::Internal(format!("Failed to flush IPC command: {}", e)))?;
            }
        }
        Ok(())
    }

    /// 启动 IPC 读取循环（后台 std 线程读 stdout → tokio 任务更新状态）
    fn start_ipc_reader(&self, stdout: std::process::ChildStdout) {
        let inner_arc = self.inner.clone();

        // 使用 std 线程读取 stdout（BufRead 是阻塞操作）
        std::thread::spawn(move || {
            use std::io::BufRead;

            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line_str) => {
                        let trimmed = line_str.trim().to_string();
                        if trimmed.is_empty() {
                            continue;
                        }

                        match IpcResponse::from_json_line(&trimmed) {
                            Ok(response) => {
                                // 使用 tokio runtime 在异步上下文中更新状态
                                let rt = tokio::runtime::Handle::current();
                                let inner = inner_arc.clone();
                                rt.spawn(async move {
                                    handle_ipc_response(inner, response).await;
                                });
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse IPC response: {} (line: {})", e, trimmed);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("IPC stdout read error: {}", e);
                        break;
                    }
                }
            }

            // stdout 关闭意味着子进程退出
            tracing::info!("IPC reader: stdout closed, child process likely exited");
            let rt = tokio::runtime::Handle::current();
            let inner = inner_arc.clone();
            rt.spawn(async move {
                let mut inner = inner.write().await;
                inner.status = ServerStatus::Stopped;
                inner.child = None;
                inner.child_stdin = None;
            });
        });
    }

    /// 启动子进程崩溃监控
    fn start_process_monitor(&self) {
        let inner_arc = self.inner.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                let mut inner = inner_arc.write().await;
                if let Some(ref mut child) = inner.child {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            tracing::warn!("Server child process exited: {}", status);
                            inner.status = ServerStatus::Stopped;
                            inner.child = None;
                            inner.child_stdin = None;
                            break;
                        }
                        Ok(None) => {
                            // 仍在运行
                        }
                        Err(e) => {
                            tracing::error!("Failed to check server process: {}", e);
                            inner.status = ServerStatus::Stopped;
                            inner.child = None;
                            inner.child_stdin = None;
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

/// 处理从子进程接收到的 IPC 响应
async fn handle_ipc_response(inner: Arc<RwLock<SupervisorInner>>, response: IpcResponse) {
    match response {
        IpcResponse::Started { port } => {
            let mut inner = inner.write().await;
            inner.status = ServerStatus::Running;
            tracing::info!("Server child process confirmed started on port {}", port);
        }
        IpcResponse::Stopped => {
            let mut inner = inner.write().await;
            inner.status = ServerStatus::Stopped;
            inner.child = None;
            inner.child_stdin = None;
            tracing::info!("Server child process confirmed stopped via IPC");
        }
        IpcResponse::Heartbeat(metrics) => {
            let mut inner = inner.write().await;
            // 追加到指标历史
            let entry = TimestampedMetrics {
                timestamp_secs: metrics.uptime_secs,
                ws_sent_rate: metrics.ws_sent_rate,
                ws_recv_rate: metrics.ws_recv_rate,
            };
            inner.metrics_history.push_back(entry);
            if inner.metrics_history.len() > 60 {
                inner.metrics_history.pop_front();
            }
            inner.metrics = *metrics;
        }
        IpcResponse::Metrics(metrics) => {
            let mut inner = inner.write().await;
            inner.metrics = *metrics;
        }
        IpcResponse::Error { message } => {
            tracing::error!("Server child process reported error: {}", message);
            let mut inner = inner.write().await;
            inner.status = ServerStatus::Stopped;
            inner.child = None;
            inner.child_stdin = None;
        }
    }
}
