//! Session Management
//!
//! 提供会话状态管理、持久化和恢复功能

use crate::db::{Database, SessionConfig as DbSessionConfig};
use crate::pty::{PtyOutputEvent, PtySession, SessionLaunchConfig};
use crate::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

/// 会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// 正在启动
    Starting,
    /// 运行中
    Running,
    /// 等待输入
    WaitingInput,
    /// 已停止
    Stopped,
    /// 出错
    Error,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// 运行时 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub config_id: String,
    pub name: String,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
}

impl SessionInfo {
    pub fn new(config_id: &str, name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config_id: config_id.to_string(),
            name: name.to_string(),
            status: SessionStatus::Starting,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
        }
    }
}

/// Session Manager
pub struct SessionManager {
    /// 运行中的 PTY 会话
    pty_sessions: Arc<RwLock<HashMap<String, PtySession>>>,
    /// 会话信息
    session_info: Arc<RwLock<HashMap<String, SessionInfo>>>,
    /// 数据库
    db: Arc<Mutex<Database>>,
    /// 全局输出广播
    output_tx: broadcast::Sender<PtyOutputEvent>,
    /// 运行标志
    running: Arc<AtomicBool>,
}

impl SessionManager {
    /// 创建新的 Session Manager
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        let (output_tx, _) = broadcast::channel(2048);
        let running = Arc::new(AtomicBool::new(true));

        Self {
            pty_sessions: Arc::new(RwLock::new(HashMap::new())),
            session_info: Arc::new(RwLock::new(HashMap::new())),
            db,
            output_tx,
            running,
        }
    }

    /// 创建新的 Session Manager (从 Database 值)
    pub fn from_database(db: Database) -> Self {
        Self::new(Arc::new(Mutex::new(db)))
    }

    /// 从配置创建会话
    pub async fn create_session(&self, config_id: &str) -> Result<String> {
        // 从数据库加载配置
        let db = self.db.lock().await;
        let config = db
            .get_session_config(config_id)?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;
        drop(db);

        // 构建启动配置
        let launch_config = self.build_launch_config(&config)?;

        // 创建 PTY 会话
        let pty_session = PtySession::new(launch_config.clone())?;
        let session_id = pty_session.id().to_string();

        // 订阅输出并转发到全局广播
        let mut rx = pty_session.subscribe_output();
        let output_tx = self.output_tx.clone();
        let session_id_clone = session_id.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match rx.recv().await {
                    Ok(event) => {
                        let _ = output_tx.send(event);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Output channel closed for session: {}", session_id_clone);
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Output channel lagged {} messages for session: {}", n, session_id_clone);
                    }
                }
            }
            tracing::debug!("Output forwarder stopped for session: {}", session_id_clone);
        });

        // 启动 PTY
        pty_session.start().await?;

        // 创建会话信息
        let info = SessionInfo::new(config_id, &config.name);

        // 保存到数据库
        let db = self.db.lock().await;
        db.add_history(config_id, &config.name, None)?;
        drop(db);

        // 保存到内存
        {
            let mut sessions = self.pty_sessions.write().await;
            sessions.insert(session_id.clone(), pty_session);
        }
        {
            let mut info_map = self.session_info.write().await;
            info_map.insert(session_id.clone(), info);
        }

        tracing::info!("Session created: {} ({})", config.name, session_id);
        Ok(session_id)
    }

    /// 从配置构建启动配置
    fn build_launch_config(&self, config: &DbSessionConfig) -> Result<SessionLaunchConfig> {
        use crate::pty::{ExecutionEnvironment, WindowsShell};

        let environment = match config.environment.as_str() {
            "wsl2" => ExecutionEnvironment::Wsl2 {
                distro: config.wsl_distro.clone().unwrap_or_else(|| "Ubuntu".to_string()),
            },
            _ => ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
        };

        Ok(SessionLaunchConfig {
            name: config.name.clone(),
            environment,
            working_dir: config.working_dir.clone(),
            command: config.command.clone(),
            env_vars: HashMap::new(),
            tmux_session: config.tmux_session.clone(),
            cols: 120,
            rows: 40,
        })
    }

    /// 获取会话
    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let info_map = self.session_info.read().await;
        info_map.get(session_id).cloned()
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let info_map = self.session_info.read().await;
        info_map.values().cloned().collect()
    }

    /// 向会话写入输入
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        let sessions = self.pty_sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;

        session.write_str(data).await?;

        // 更新状态
        let mut info_map = self.session_info.write().await;
        if let Some(info) = info_map.get_mut(session_id) {
            info.status = SessionStatus::Running;
        }

        Ok(())
    }

    /// 发送特殊键
    pub async fn send_special_key(&self, session_id: &str, key: &str) -> Result<()> {
        let sessions = self.pty_sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;

        session.send_special_key(key).await?;
        Ok(())
    }

    /// 调整会话终端大小
    pub async fn resize_session(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        let sessions = self.pty_sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;

        session.resize(cols, rows).await?;
        Ok(())
    }

    /// 终止会话
    pub async fn kill_session(&self, session_id: &str) -> Result<()> {
        // 终止 PTY
        {
            let sessions = self.pty_sessions.read().await;
            if let Some(session) = sessions.get(session_id) {
                session.kill().await?;
            }
        }

        // 移除会话
        {
            let mut sessions = self.pty_sessions.write().await;
            sessions.remove(session_id);
        }

        // 更新状态
        {
            let mut info_map = self.session_info.write().await;
            if let Some(info) = info_map.get_mut(session_id) {
                info.status = SessionStatus::Stopped;
                info.stopped_at = Some(Utc::now());
            }
        }

        tracing::info!("Session killed: {}", session_id);
        Ok(())
    }

    /// 订阅全局输出
    pub fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent> {
        self.output_tx.subscribe()
    }

    /// 获取会话状态
    pub async fn get_session_status(&self, session_id: &str) -> Option<SessionStatus> {
        let info_map = self.session_info.read().await;
        info_map.get(session_id).map(|i| i.status)
    }

    /// 更新会话状态
    pub async fn update_session_status(&self, session_id: &str, status: SessionStatus) {
        let mut info_map = self.session_info.write().await;
        if let Some(info) = info_map.get_mut(session_id) {
            info.status = status;
        }
    }

    /// 检测等待输入状态
    pub async fn detect_waiting_input(&self, session_id: &str, output: &str) -> bool {
        let waiting = crate::parser::detect_waiting_input(output);

        if waiting {
            self.update_session_status(session_id, SessionStatus::WaitingInput).await;
        }

        waiting
    }

    /// 清理已停止的会话
    pub async fn cleanup_stopped_sessions(&self) {
        let mut sessions = self.pty_sessions.write().await;
        let mut info_map = self.session_info.write().await;

        let stopped_ids: Vec<String> = info_map
            .iter()
            .filter(|(_, info)| info.status == SessionStatus::Stopped)
            .map(|(id, _)| id.clone())
            .collect();

        for id in stopped_ids {
            sessions.remove(&id);
            info_map.remove(&id);
            tracing::debug!("Cleaned up stopped session: {}", id);
        }
    }

    /// 关闭 SessionManager，停止所有会话
    pub async fn shutdown(&self) {
        tracing::info!("SessionManager shutting down...");
        self.running.store(false, Ordering::SeqCst);

        // 终止所有会话
        let sessions = self.pty_sessions.read().await;
        for (id, session) in sessions.iter() {
            if let Err(e) = session.kill().await {
                tracing::error!("Failed to kill session {}: {}", id, e);
            }
        }

        tracing::info!("SessionManager shutdown complete");
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        // 创建内存数据库用于测试
        let db = Database::new(std::path::Path::new(":memory:"))
            .expect("Failed to create memory database");
        db.init_schema().expect("Failed to init schema");
        Self::from_database(db)
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
