//! PTY Process Management
//!
//! 封装 portable-pty，提供跨平台的 PTY 管理功能
//! 核心职责：PTY 会话的生命周期管理（创建、启动、终止、resize）

use crate::desktop::enums::{PtySessionStatus, SessionLaunchConfig};
use crate::desktop::model::PtyOutputEvent;
use crate::desktop::pty::command::build_command;
use crate::desktop::pty::pty_reader::PtyReader;
use crate::desktop::traits::PtyOutputListener;
use crate::shared::system::config::AppConfig;
use crate::shared::system::process::create_command;
use crate::Result;

use portable_pty::{native_pty_system, PtyPair, PtySize};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;



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
    /// 输出事件监听器列表（观察者模式）
    /// 使用 Mutex 保护，允许跨线程访问
    /// 支持存储异步 PtyOutputListener (AsyncPtyOutputListener 实现了该 trait)
    output_listeners: Arc<Mutex<Vec<Arc<dyn PtyOutputListener>>>>,
    /// 生命周期事件发送器（进程退出、错误等）
    pub lifecycle_tx: broadcast::Sender<PtySessionStatus>,
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
    /// 生命周期事件发送器的共享引用
    lifecycle_tx: broadcast::Sender<PtySessionStatus>,
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
        Self::with_id(id, config)
    }

    /// 使用指定 ID 创建 PTY 会话（用于重启时复用旧 ID）
    pub fn with_id(id: String, config: SessionLaunchConfig) -> Result<Self> {
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
        let (lifecycle_tx, _) = broadcast::channel(AppConfig::global().channels.lifecycle_capacity);

        let running = Arc::new(AtomicBool::new(true));

        let state = PtySessionState {
            id: id.clone(),
            name: config.name.clone(),
            config,
            pair: Some(pair),
            writer: Some(writer),
            running: running.clone(),
            output_listeners: Arc::new(Mutex::new(Vec::new())),
            lifecycle_tx: lifecycle_tx.clone(),
            reader_handle: None,
            process_id: None,
        };

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            running,
            lifecycle_tx,
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
            let mut cmd = build_command(&state.config)?;

            // 注入 BedCode session ID 到进程环境变量，
            // 让 Claude Code hooks 能关联到 BedCode 的 PTY 会话
            cmd.env("BEDCODE_SESSION_ID", &self.id);

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

    /// 写入输入
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        let mut state = self.state.lock().await;
        let writer = state.writer.as_mut()
            .ok_or_else(|| {
                tracing::error!("[PtyProcess] write: writer not available");
                crate::AppError::Pty("Writer not available".to_string())
            })?;

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
        let combo = crate::shared::enums::KeyCombo::parse(key)
            .ok_or_else(|| crate::AppError::InvalidInput(format!("Unknown special key: {}", key)))?;

        let bytes = combo.to_pty_bytes()
            .ok_or_else(|| crate::AppError::InvalidInput(format!("Unsupported key combo: {}", key)))?;

        self.write(&bytes).await
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

        Ok(())
    }

    /// 添加输出事件监听器（观察者模式）
    ///
    /// 外部实现 PtyOutputListener trait 来接收输出事件
    pub fn add_output_listener(&self, listener: Arc<dyn PtyOutputListener>) {
        if let Ok(mut state) = self.state.try_lock() {
            if let Ok(mut listeners) = state.output_listeners.try_lock() {
                listeners.push(listener);
            }
        }
    }

    /// 通知所有监听器（内部方法，在输出产生时调用）
    fn notify_listeners(&self, event: PtyOutputEvent) {
        let listeners = {
            if let Ok(state) = self.state.try_lock() {
                if let Ok(listeners) = state.output_listeners.try_lock() {
                    Some(listeners.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(listeners) = listeners {
            for listener in listeners {
                listener.on_output(event.clone());
            }
        }
    }

    /// 订阅生命周期事件（进程退出、错误等）
    pub fn subscribe_lifecycle(&self) -> broadcast::Receiver<PtySessionStatus> {
        self.lifecycle_tx.subscribe()
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
            #[cfg(target_os = "windows")]
            {
                tracing::info!("Executing taskkill for PID {}", pid);
                let output = create_command("cmd")
                    .args(["/C", &format!("taskkill /F /T /PID {}", pid)])
                    .output();
                match output {
                    Ok(o) => tracing::info!("taskkill output: {}", String::from_utf8_lossy(&o.stdout)),
                    Err(e) => tracing::error!("taskkill failed: {}", e),
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
        } else {
            tracing::warn!("No process_id available for session {}", self.id);
        }

        tracing::info!("PTY session killed: {} (pid={:?})", self.id, pid);
        Ok(())
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

        // 使用 PtyReader（观察者模式）
        let output_listeners = {
            let state: tokio::sync::MutexGuard<'_, PtySessionState> = self.state.lock().await;
            state.output_listeners.clone()
        };

        let pty_reader = PtyReader::start(
            reader,
            output_listeners,
            self.lifecycle_tx.clone(),
            self.id.clone(),
            self.running.clone(),
        );

        // 保存线程句柄
        {
            let mut state = self.state.lock().await;
            state.reader_handle = Some(pty_reader.into_inner());
        }

        Ok(())
    }
}

impl Clone for PtySession {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            running: self.running.clone(),
            lifecycle_tx: self.lifecycle_tx.clone(),
            id: self.id.clone(),
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Only stop if this is the last reference
        if Arc::strong_count(&self.state) == 1 {
            self.running.store(false, Ordering::SeqCst);

            // 尝试终止进程（同步方式，因为 Drop 不能是 async）
            if let Ok(state) = self.state.try_lock() {
                if let Some(pid) = state.process_id {
                    #[cfg(target_os = "windows")]
                    {
                        let _ = create_command("cmd")
                            .args(["/C", &format!("taskkill /F /T /PID {}", pid)])
                            .output();
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        let _ = std::process::Command::new("kill")
                            .args(["-9", &pid.to_string()])
                            .output();
                    }
                    tracing::info!("PTY session killed on drop: {} (pid={})", self.id, pid);
                }
            }
        }
    }
}