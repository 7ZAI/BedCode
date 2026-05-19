//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布

use super::{PtySessionHandler, SessionStorage};
use super::storage::SessionStore;
use super::pty_handler::PtyHandler;
use crate::desktop::pty::{PtyOutputEvent, PtySession, SessionLaunchConfig};
use crate::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Session Manager
///
/// 使用依赖注入模式，通过具体类型获取服务
/// - SessionStorage: 负责会话配置的数据库操作
/// - PtySessionHandler: 负责 PTY 生命周期管理
pub struct SessionManager {
    /// 运行中的 PTY 会话
    pty_sessions: Arc<RwLock<HashMap<String, PtySession>>>,
    /// 会话信息
    session_info: Arc<RwLock<HashMap<String, super::SessionInfo>>>,
    /// 会话存储（数据库操作）
    storage: Arc<SessionStorage>,
    /// PTY 处理器
    pty_handler: Arc<PtySessionHandler>,
    /// 全局输出广播
    output_tx: broadcast::Sender<PtyOutputEvent>,
    /// 会话状态变化广播
    status_tx: broadcast::Sender<super::SessionStatusEvent>,
    /// 会话重启广播
    restart_tx: broadcast::Sender<super::SessionRestartEvent>,
    /// 运行标志
    running: Arc<AtomicBool>,
}

impl SessionManager {
    /// 获取输出广播发送器
    pub fn output_tx(&self) -> broadcast::Sender<PtyOutputEvent> {
        self.output_tx.clone()
    }

    /// 获取会话状态变化广播发送器
    pub fn status_tx(&self) -> broadcast::Sender<super::SessionStatusEvent> {
        self.status_tx.clone()
    }

    /// 获取会话重启广播发送器
    pub fn restart_tx(&self) -> broadcast::Sender<super::SessionRestartEvent> {
        self.restart_tx.clone()
    }

    /// 创建新的 Session Manager（使用具体实现）
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        let pty_handler = Arc::new(PtySessionHandler::new());
        Self::new_with_handlers(storage, pty_handler)
    }

    /// 从数据库创建 Session Manager（兼容旧 API）
    pub fn from_database(db: crate::shared::db::Database) -> Self {
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(SessionStorage::new(db));
        let pty_handler = Arc::new(PtySessionHandler::new());
        Self::new_with_handlers(storage, pty_handler)
    }

    /// 创建新的 Session Manager（使用具体类型注入）
    pub fn new_with_handlers(
        storage: Arc<SessionStorage>,
        pty_handler: Arc<PtySessionHandler>,
    ) -> Self {
        let (output_tx, _) = broadcast::channel(2048);
        let (status_tx, _) = broadcast::channel(64);
        let (restart_tx, _) = broadcast::channel(64);
        let running = Arc::new(AtomicBool::new(true));

        Self {
            pty_sessions: Arc::new(RwLock::new(HashMap::new())),
            session_info: Arc::new(RwLock::new(HashMap::new())),
            storage,
            pty_handler,
            output_tx,
            status_tx,
            restart_tx,
            running,
        }
    }

    /// 从配置创建会话
    pub async fn create_session(&self, config_id: &str) -> Result<String> {
        // 从存储加载配置
        let config = self
            .storage
            .get_config(config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        // 生成唯一的会话名称
        let session_name = self
            .generate_unique_name(config_id, &config.name)
            .await;

        // 构建启动配置
        let launch_config = self.build_launch_config(&config)?;

        // 创建 PTY 会话
        let pty_session = self.pty_handler.create_session(launch_config.clone())?;
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
                        tracing::debug!(
                            "Output channel closed for session: {}",
                            session_id_clone
                        );
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "Output channel lagged {} messages for session: {}",
                            n,
                            session_id_clone
                        );
                    }
                }
            }
            tracing::debug!("Output forwarder stopped for session: {}", session_id_clone);
        });

        // 启动 PTY
        pty_session.start().await?;

        // 订阅生命周期事件，进程退出时自动更新会话状态
        let mut lifecycle_rx = pty_session.subscribe_lifecycle();
        let session_id_lifecycle = session_id.clone();
        let session_info_ref = self.session_info.clone();
        let status_tx = self.status_tx.clone();
        tokio::spawn(async move {
            match lifecycle_rx.recv().await {
                Ok(status) => {
                    tracing::info!(
                        "Session {} lifecycle event: {:?}, updating status",
                        session_id_lifecycle, status
                    );
                    let session_status = match status {
                        crate::desktop::pty::PtySessionStatus::Error => super::SessionStatus::Error(None),
                        _ => super::SessionStatus::Stopped,
                    };
                    let session_name = {
                        let info_map = session_info_ref.read().await;
                        info_map
                            .get(&session_id_lifecycle)
                            .map(|i| i.name.clone())
                            .unwrap_or_default()
                    };
                    {
                        let mut info_map = session_info_ref.write().await;
                        if let Some(info) = info_map.get_mut(&session_id_lifecycle) {
                            let old_status = info.status.clone();
                            info.status = session_status.clone();

                            // 发送状态变化事件
                            let _ = status_tx.send(super::SessionStatusEvent {
                                session_id: session_id_lifecycle.clone(),
                                old_status: Some(old_status),
                                new_status: session_status,
                                session_name,
                            });
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(
                        "Lifecycle channel closed for session: {} ({})",
                        session_id_lifecycle, e
                    );
                }
            }
        });

        // 创建会话信息
        let info = super::SessionInfo {
            id: session_id.clone(),
            config_id: config_id.to_string(),
            name: session_name.clone(),
            status: super::SessionStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            stopped_at: None,
            session_type: super::SessionType::Pty,
        };

        // 保存到内存
        {
            let mut sessions = self.pty_sessions.write().await;
            sessions.insert(session_id.clone(), pty_session);
        }
        {
            let mut info_map = self.session_info.write().await;
            info_map.insert(session_id.clone(), info);
        }

        tracing::info!("Session created: {} ({})", session_name, session_id);
        Ok(session_id)
    }

    /// 生成唯一的会话名称
    async fn generate_unique_name(&self, config_id: &str, base_name: &str) -> String {
        let info_map = self.session_info.read().await;

        let count = info_map
            .values()
            .filter(|s| s.config_id == config_id && s.status != super::SessionStatus::Stopped)
            .count();

        if count == 0 {
            base_name.to_string()
        } else {
            format!("{}({})", base_name, count)
        }
    }

    /// 重启会话
    pub async fn restart_session(&self, session_id: &str) -> Result<String> {
        let (config_id, old_name) = {
            let info_map = self.session_info.read().await;
            let info = info_map
                .get(session_id)
                .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?;
            (info.config_id.clone(), info.name.clone())
        };

        let _ = self.remove_session(session_id).await;

        let config = self
            .storage
            .get_config(&config_id)
            .await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        let mut launch_config = self.build_launch_config(&config)?;
        launch_config.name = old_name.clone();

        let old_name_for_info = old_name.clone();
        let old_name_for_event = old_name.clone();

        let pty_session = self
            .pty_handler
            .create_session_with_id(session_id.to_string(), launch_config.clone())?;

        let mut rx = pty_session.subscribe_output();
        let output_tx = self.output_tx.clone();
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
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Output channel lagged {} messages", n);
                    }
                }
            }
        });

        let mut lifecycle_rx = pty_session.subscribe_lifecycle();
        let session_info_ref = self.session_info.clone();
        let status_tx = self.status_tx.clone();
        let session_id_for_lifecycle = session_id.to_string();
        tokio::spawn(async move {
            if let Ok(status) = lifecycle_rx.recv().await {
                let session_status = match status {
                    crate::desktop::pty::PtySessionStatus::Error => super::SessionStatus::Error(None),
                    _ => super::SessionStatus::Stopped,
                };
                let mut info_map = session_info_ref.write().await;
                if let Some(info) = info_map.get_mut(&session_id_for_lifecycle) {
                    let old_status = info.status.clone();
                    info.status = session_status.clone();
                    let _ = status_tx.send(super::SessionStatusEvent {
                        session_id: session_id_for_lifecycle,
                        old_status: Some(old_status),
                        new_status: session_status,
                        session_name: info.name.clone(),
                    });
                }
            }
        });

        pty_session.start().await?;

        let info = super::SessionInfo {
            id: session_id.to_string(),
            config_id: config_id.clone(),
            name: old_name_for_info,
            status: super::SessionStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            stopped_at: None,
            session_type: super::SessionType::Pty,
        };

        {
            let mut sessions = self.pty_sessions.write().await;
            sessions.insert(session_id.to_string(), pty_session);
        }
        {
            let mut info_map = self.session_info.write().await;
            info_map.insert(session_id.to_string(), info);
        }

        tracing::info!(
            "Session restarted: {} ({})",
            old_name_for_event, session_id
        );

        let _ = self.restart_tx.send(super::SessionRestartEvent {
            old_session_id: session_id.to_string(),
            new_session_id: session_id.to_string(),
            session_name: old_name,
        });

        Ok(session_id.to_string())
    }

    /// 从配置构建启动配置
    fn build_launch_config(
        &self,
        config: &crate::shared::db::SessionConfig,
    ) -> Result<SessionLaunchConfig> {
        use crate::desktop::pty::{ExecutionEnvironment, WindowsShell};

        let environment = match config.environment.as_str() {
            "wsl2" => ExecutionEnvironment::Wsl2 {
                distro: config
                    .wsl_distro
                    .clone()
                    .unwrap_or_else(|| "Ubuntu".to_string()),
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
            env_vars: std::collections::HashMap::new(),
            tmux_session: config.tmux_session.clone(),
            cols: 120,
            rows: 40,
        })
    }

    /// 获取会话
    pub async fn get_session(&self, session_id: &str) -> Option<super::SessionInfo> {
        let info_map = self.session_info.read().await;
        info_map.get(session_id).cloned()
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> Vec<super::SessionInfo> {
        let info_map = self.session_info.read().await;
        info_map.values().cloned().collect()
    }

    /// 向会话写入输入
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        tracing::info!("[SessionManager] write_input session_id={}, data_len={}, data={:?}",
            session_id, data.len(), &data[..data.len().min(50)]);
        let sessions = self.pty_sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| {
                tracing::error!("[SessionManager] write_input: session not found: {}", session_id);
                crate::AppError::NotFound(format!("Session not found: {}", session_id))
            })?;

        session.write_str(data).await?;

        let mut info_map = self.session_info.write().await;
        if let Some(info) = info_map.get_mut(session_id) {
            info.status = super::SessionStatus::Running;
        }

        tracing::info!("[SessionManager] write_input OK session_id={}", session_id);
        Ok(())
    }

    /// 发送特殊键
    pub async fn send_special_key(&self, session_id: &str, key: &str) -> Result<()> {
        tracing::info!("[SessionManager] send_special_key session_id={}, key={:?}", session_id, key);
        let sessions = self.pty_sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| {
                tracing::error!("[SessionManager] send_special_key: session not found: {}", session_id);
                crate::AppError::NotFound(format!("Session not found: {}", session_id))
            })?;

        session.send_special_key(key).await?;
        tracing::info!("[SessionManager] send_special_key OK session_id={}", session_id);
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
        tracing::info!("kill_session called for: {}", session_id);

        {
            let sessions = self.pty_sessions.read().await;
            if let Some(session) = sessions.get(session_id) {
                session.kill().await?;
                tracing::info!("PTY killed for session: {}", session_id);
            } else {
                tracing::warn!("Session not found in pty_sessions: {}", session_id);
            }
        }

        {
            let mut sessions = self.pty_sessions.write().await;
            sessions.remove(session_id);
            tracing::info!("Session removed from pty_sessions: {}", session_id);
        }

        {
            let mut info_map = self.session_info.write().await;
            if let Some(info) = info_map.get_mut(session_id) {
                let old_status = info.status.clone();
                info.status = super::SessionStatus::Stopped;
                info.stopped_at = Some(Utc::now());
                tracing::info!("Session status updated to Stopped: {}", session_id);

                let _ = self.status_tx.send(super::SessionStatusEvent {
                    session_id: session_id.to_string(),
                    old_status: Some(old_status),
                    new_status: super::SessionStatus::Stopped,
                    session_name: info.name.clone(),
                });
            } else {
                tracing::warn!("Session not found in session_info: {}", session_id);
            }
        }

        tracing::info!("Session killed: {}", session_id);
        Ok(())
    }

    /// 删除会话
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("remove_session called for: {}", session_id);

        {
            let mut sessions = self.pty_sessions.write().await;
            if let Some(session) = sessions.remove(session_id) {
                session.kill().await?;
                tracing::info!("PTY killed for removed session: {}", session_id);
            }
        }

        {
            let mut info_map = self.session_info.write().await;
            info_map.remove(session_id);
            tracing::info!("Session removed from info map: {}", session_id);
        }

        Ok(())
    }

    /// 订阅全局输出
    pub fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent> {
        self.output_tx.subscribe()
    }

    /// 订阅会话状态变化
    pub fn subscribe_status(&self) -> broadcast::Receiver<super::SessionStatusEvent> {
        self.status_tx.subscribe()
    }

    /// 订阅会话重启
    pub fn subscribe_restart(&self) -> broadcast::Receiver<super::SessionRestartEvent> {
        self.restart_tx.subscribe()
    }

    /// 获取会话状态
    pub async fn get_session_status(&self, session_id: &str) -> Option<super::SessionStatus> {
        let info_map = self.session_info.read().await;
        info_map.get(session_id).map(|i| i.status.clone())
    }

    /// 更新会话状态
    pub async fn update_session_status(&self, session_id: &str, status: super::SessionStatus) {
        let mut info_map = self.session_info.write().await;
        if let Some(info) = info_map.get_mut(session_id) {
            info.status = status;
        }
    }

    /// 检测等待输入状态
    pub async fn detect_waiting_input(&self, session_id: &str, output: &str) -> bool {
        let waiting = crate::shared::parser::detect_waiting_input(output);

        if waiting {
            self.update_session_status(session_id, super::SessionStatus::WaitingInput)
                .await;
        }

        waiting
    }

    /// 清理已停止的会话
    pub async fn cleanup_stopped_sessions(&self) {
        let mut sessions = self.pty_sessions.write().await;
        let mut info_map = self.session_info.write().await;

        let stopped_ids: Vec<String> = info_map
            .iter()
            .filter(|(_, info)| info.status == super::SessionStatus::Stopped)
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
        let db = crate::shared::db::Database::new(std::path::Path::new(":memory:"))
            .expect("Failed to create memory database");
        db.init_schema().expect("Failed to init schema");

        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(SessionStorage::new(db));
        let pty_handler = Arc::new(PtySessionHandler::new());

        Self::new_with_handlers(storage, pty_handler)
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