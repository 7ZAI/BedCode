//! PTY Process Management
//!
//! 封装 portable-pty，提供跨平台的 PTY 管理功能

use super::{ExecutionEnvironment, SessionLaunchConfig, WindowsShell};
use crate::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

/// PTY 输出事件
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutputEvent {
    pub session_id: String,
    pub data: String, // Base64 encoded
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// PTY 会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PtySessionStatus {
    Starting,
    Running,
    WaitingInput,
    Stopped,
    Error,
}

/// PTY 会话内部状态
///
/// 使用 Arc<Mutex<>> 包装所有非 Send/Sync 的类型，确保线程安全
pub struct PtySessionState {
    /// 会话 ID
    pub id: String,
    /// 会话名称
    pub name: String,
    /// 启动配置
    pub config: SessionLaunchConfig,
    /// 运行标志
    pub running: Arc<AtomicBool>,
    /// PTY pair (portable-pty 的核心结构)
    pub pair: Option<PtyPair>,
    /// 写入器
    pub writer: Option<Box<dyn Write + Send>>,
    /// 输出事件发送器
    pub output_tx: broadcast::Sender<PtyOutputEvent>,
    /// 读取线程句柄
    pub reader_handle: Option<JoinHandle<()>>,
    /// 进程 ID（用于强制终止）
    pub process_id: Option<u32>,
}

/// PTY 会话 - 线程安全的包装器
///
/// 所有内部状态都通过 Arc<Mutex<>> 保护，自动实现 Send + Sync
pub struct PtySession {
    state: Arc<Mutex<PtySessionState>>,
    /// 运行标志的共享引用（用于快速检查）
    running: Arc<AtomicBool>,
    /// 输出事件发送器的共享引用
    output_tx: broadcast::Sender<PtyOutputEvent>,
    /// 会话 ID 的缓存（避免频繁加锁）
    id: String,
}

// 自动派生 Send + Sync，因为所有内部字段都是线程安全的
// Arc<Mutex<T>> 是 Send + Sync (当 T: Send)
// Arc<AtomicBool> 是 Send + Sync
// broadcast::Sender 是 Send + Sync
// String 是 Send + Sync

impl PtySession {
    /// 创建新的 PTY 会话
    pub fn new(config: SessionLaunchConfig) -> Result<Self> {
        let id = Uuid::new_v4().to_string();
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| crate::AppError::Pty(e.to_string()))?;

        let writer = pair.master.take_writer()
            .map_err(|e| crate::AppError::Pty(e.to_string()))?;
        let (output_tx, _) = broadcast::channel(1024);

        let running = Arc::new(AtomicBool::new(true));

        let state = PtySessionState {
            id: id.clone(),
            name: config.name.clone(),
            config,
            pair: Some(pair),
            writer: Some(writer),
            running: running.clone(),
            output_tx: output_tx.clone(),
            reader_handle: None,
            process_id: None,
        };

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            running,
            output_tx,
            id,
        })
    }

    /// 获取会话 ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 获取会话名称
    pub async fn name(&self) -> String {
        let state = self.state.lock().await;
        state.name.clone()
    }

    /// 启动 PTY 会话
    pub async fn start(&self) -> Result<()> {
        let (cmd, pair) = {
            let mut state = self.state.lock().await;
            let cmd = self.build_command(&state.config)?;

            // 从 state 中取出 pair
            let pair = state.pair.take()
                .ok_or_else(|| crate::AppError::Pty("PTY pair already used".to_string()))?;

            (cmd, pair)
        };

        let child = pair.slave
            .spawn_command(cmd)
            .map_err(|e| crate::AppError::Pty(e.to_string()))?;

        // 获取进程 ID 用于后续强制终止
        let pid = child.process_id();

        // 将 pair 放回 state，并保存进程 ID
        {
            let mut state = self.state.lock().await;
            state.pair = Some(pair);
            state.process_id = pid;
        }

        // 启动输出读取线程
        self.start_output_reader().await?;

        tracing::info!("PTY session started: {} ({}, pid={:?})", self.id, self.id, pid);
        Ok(())
    }

    /// 构建命令
    fn build_command(&self, config: &SessionLaunchConfig) -> Result<CommandBuilder> {
        let mut cmd = match &config.environment {
            ExecutionEnvironment::Windows { shell } => {
                match shell {
                    WindowsShell::PowerShell => {
                        // 构建完整的 PowerShell 命令：
                        // 1. 切换到指定目录
                        // 2. 显示当前路径（让用户确认）
                        // 3. 执行用户配置的命令
                        let full_command = format!(
                            "Set-Location '{}'; Write-Host 'Working directory:' $PWD.Path; {}",
                            config.working_dir,
                            config.command
                        );

                        let mut cmd = CommandBuilder::new("powershell.exe");
                        cmd.arg("-NoLogo");
                        cmd.arg("-NoExit");
                        cmd.arg("-Command");
                        cmd.arg(full_command);
                        cmd
                    }
                    WindowsShell::Cmd => {
                        // Cmd 类似处理
                        let full_command = format!(
                            "cd /d \"{}\" && echo Working directory: %cd% && {}",
                            config.working_dir,
                            config.command
                        );

                        let mut cmd = CommandBuilder::new("cmd.exe");
                        cmd.arg("/K");
                        cmd.arg(full_command);
                        cmd
                    }
                }
            }
            ExecutionEnvironment::Wsl2 { distro } => {
                let mut cmd = CommandBuilder::new("wsl.exe");
                cmd.arg("-d");
                cmd.arg(distro);
                cmd.arg("--");
                cmd.arg("bash");
                cmd.arg("-lic");

                // 构建在 WSL 中执行的命令：
                // 1. 切换到指定目录
                // 2. 显示当前路径
                // 3. 执行用户命令
                // -l 使 bash 加载登录配置（包括 PATH）
                // -i 使 bash 交互式运行
                let wsl_command = format!(
                    "cd '{}' && pwd && {}",
                    config.working_dir.replace('\\', "/"),
                    config.command
                );
                cmd.arg(wsl_command);
                cmd
            }
        };

        // 设置进程工作目录（作为备选，确保进程启动位置正确）
        if matches!(config.environment, ExecutionEnvironment::Windows { .. }) {
            cmd.cwd(&config.working_dir);
        }

        // 设置环境变量
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        Ok(cmd)
    }

    /// 启动输出读取线程
    async fn start_output_reader(&self) -> Result<()> {
        let reader = {
            let mut state = self.state.lock().await;
            let pair = state.pair.as_mut()
                .ok_or_else(|| crate::AppError::Pty("PTY pair not available".to_string()))?;

            pair.master.try_clone_reader()
                .map_err(|e| crate::AppError::Pty(e.to_string()))?
        };

        let mut buf_reader = BufReader::new(reader);
        let output_tx = self.output_tx.clone();
        let session_id = self.id.clone();
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let mut buffer = [0u8; 4096];

            while running.load(Ordering::SeqCst) {
                match buf_reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - process exited
                        tracing::info!("PTY session ended: {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        let event = PtyOutputEvent {
                            session_id: session_id.clone(),
                            data: base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                &buffer[..n],
                            ),
                            timestamp: chrono::Utc::now(),
                        };

                        if output_tx.send(event).is_err() {
                            tracing::debug!("No output subscribers for session: {}", session_id);
                        }
                    }
                    Err(e) => {
                        tracing::error!("PTY read error: {}", e);
                        break;
                    }
                }
            }

            tracing::debug!("Output reader stopped for session: {}", session_id);
        });

        // 保存线程句柄
        {
            let mut state = self.state.lock().await;
            state.reader_handle = Some(handle);
        }

        Ok(())
    }

    /// 写入输入
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        let mut state = self.state.lock().await;
        let writer = state.writer.as_mut()
            .ok_or_else(|| crate::AppError::Pty("Writer not available".to_string()))?;

        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    /// 写入字符串
    pub async fn write_str(&self, text: &str) -> Result<()> {
        self.write(text.as_bytes()).await
    }

    /// 发送特殊键
    pub async fn send_special_key(&self, key: &str) -> Result<()> {
        let sequence = match key.to_lowercase().as_str() {
            "enter" => "\r",
            "tab" => "\t",
            "escape" | "esc" => "\x1b",
            "ctrl_c" | "ctrlc" => "\x03",
            "ctrl_d" | "ctrld" => "\x04",
            "ctrl_z" | "ctrlz" => "\x1a",
            "backspace" => "\x7f",
            "arrow_up" | "up" => "\x1b[A",
            "arrow_down" | "down" => "\x1b[B",
            "arrow_right" | "right" => "\x1b[C",
            "arrow_left" | "left" => "\x1b[D",
            _ => return Err(crate::AppError::InvalidInput(format!("Unknown special key: {}", key))),
        };

        self.write(sequence.as_bytes()).await
    }

    /// 调整终端大小
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let mut state = self.state.lock().await;
        let pair = state.pair.as_mut()
            .ok_or_else(|| crate::AppError::Pty("PTY pair not available".to_string()))?;

        pair.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| crate::AppError::Pty(e.to_string()))?;

        tracing::debug!("PTY resized to {}x{} for session: {}", cols, rows, self.id);
        Ok(())
    }

    /// 订阅输出事件
    pub fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent> {
        self.output_tx.subscribe()
    }

    /// 获取会话状态
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 终止会话
    pub async fn kill(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);

        // 获取进程 ID
        let pid = {
            let state = self.state.lock().await;
            tracing::info!("Kill session {}: process_id = {:?}", self.id, state.process_id);
            state.process_id
        };

        // 先尝试优雅退出
        let _ = self.send_special_key("ctrl_c").await;
        let _ = self.write_str("\nexit\n").await;

        // 如果有进程 ID，强制终止进程树
        if let Some(pid) = pid {
            // 在 Windows 上使用 taskkill 强制终止进程及其子进程
            #[cfg(target_os = "windows")]
            {
                tracing::info!("Executing taskkill for PID {}", pid);
                let output = std::process::Command::new("cmd")
                    .args(["/C", &format!("taskkill /F /T /PID {}", pid)])
                    .output();
                match output {
                    Ok(o) => tracing::info!("taskkill output: {}", String::from_utf8_lossy(&o.stdout)),
                    Err(e) => tracing::error!("taskkill failed: {}", e),
                }
            }

            // 在 Linux/macOS 上使用 kill
            #[cfg(not(target_os = "windows"))]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
                tracing::debug!("Sent kill -9 to PID {}", pid);
            }
        } else {
            tracing::warn!("No process_id available for session {}", self.id);
        }

        tracing::info!("PTY session killed: {} (pid={:?})", self.id, pid);
        Ok(())
    }
}

impl Clone for PtySession {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            running: self.running.clone(),
            output_tx: self.output_tx.clone(),
            id: self.id.clone(),
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Only stop if this is the last reference
        if Arc::strong_count(&self.state) == 1 {
            self.running.store(false, Ordering::SeqCst);
            tracing::debug!("PTY session dropped: {}", self.id);
        }
    }
}
