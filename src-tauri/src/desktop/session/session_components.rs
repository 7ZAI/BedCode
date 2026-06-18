//! Session Components
//!
//! 会话管理器的内部组件：注册表、命名服务、配置映射、状态检测
//! 这些组件各自只有一个实现，trait 已内联到此文件

use crate::desktop::model::SessionInfo;
use crate::desktop::pty::{ExecutionEnvironment, PtySession, SessionLaunchConfig, WindowsShell};
use crate::shared::db::SessionConfig;
use crate::shared::enums::SessionStatus;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ==================== PTY Registry ====================

/// PTY 会话注册表 - 负责 PTY 会话的存储和基本操作
pub trait PtyRegistry: Send + Sync {
    async fn insert(&self, id: String, session: PtySession);
    async fn remove(&self, id: &str) -> Option<PtySession>;
    async fn get(&self, id: &str) -> Option<PtySession>;
    async fn list(&self) -> Vec<PtySession>;
    async fn list_ids(&self) -> Vec<String>;
    async fn write_input(&self, id: &str, data: &str) -> Result<()>;
    async fn send_special_key(&self, id: &str, key: &str) -> Result<()>;
    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()>;
    async fn kill(&self, id: &str) -> Result<()>;
    async fn kill_all(&self) -> Result<()>;
}

pub struct DefaultPtyRegistry {
    sessions: Arc<RwLock<HashMap<String, PtySession>>>,
}

impl DefaultPtyRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultPtyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyRegistry for DefaultPtyRegistry {
    async fn insert(&self, id: String, session: PtySession) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(id, session);
    }

    async fn remove(&self, id: &str) -> Option<PtySession> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id)
    }

    async fn get(&self, id: &str) -> Option<PtySession> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    async fn list(&self) -> Vec<PtySession> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    async fn list_ids(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    async fn write_input(&self, id: &str, data: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", id)))?;
        session.write_str(data).await
    }

    async fn send_special_key(&self, id: &str, key: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", id)))?;
        session.send_special_key(key).await
    }

    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", id)))?;
        session.resize(cols, rows).await
    }

    async fn kill(&self, id: &str) -> Result<()> {
        let session = self.remove(id).await;
        if let Some(s) = session {
            s.kill().await?;
        }
        Ok(())
    }

    async fn kill_all(&self) -> Result<()> {
        let sessions: Vec<(String, PtySession)> = {
            let mut map = self.sessions.write().await;
            map.drain().collect()
        };
        for (id, session) in sessions {
            if let Err(e) = session.kill().await {
                tracing::error!("Failed to kill session {}: {}", id, e);
            }
        }
        Ok(())
    }
}

// ==================== Session Info Registry ====================

/// 会话信息注册表 - 负责会话元数据的存储和状态管理
pub trait SessionInfoRegistry: Send + Sync {
    async fn insert(&self, info: SessionInfo);
    async fn remove(&self, id: &str) -> Option<SessionInfo>;
    async fn get(&self, id: &str) -> Option<SessionInfo>;
    async fn list(&self) -> Vec<SessionInfo>;
    async fn update_status(&self, id: &str, status: SessionStatus);
    async fn update_status_with_time(&self, id: &str, status: SessionStatus);
    async fn get_status(&self, id: &str) -> Option<SessionStatus>;
    async fn filter_by_config(&self, config_id: &str) -> Vec<SessionInfo>;
    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo>;
}

pub struct DefaultSessionInfoRegistry {
    info: Arc<RwLock<HashMap<String, SessionInfo>>>,
}

impl DefaultSessionInfoRegistry {
    pub fn new() -> Self {
        Self {
            info: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultSessionInfoRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionInfoRegistry for DefaultSessionInfoRegistry {
    async fn insert(&self, info: SessionInfo) {
        let mut map = self.info.write().await;
        map.insert(info.id.clone(), info);
    }

    async fn remove(&self, id: &str) -> Option<SessionInfo> {
        let mut map = self.info.write().await;
        map.remove(id)
    }

    async fn get(&self, id: &str) -> Option<SessionInfo> {
        let map = self.info.read().await;
        map.get(id).cloned()
    }

    async fn list(&self) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values().cloned().collect()
    }

    async fn update_status(&self, id: &str, status: SessionStatus) {
        let mut map = self.info.write().await;
        if let Some(info) = map.get_mut(id) {
            info.status = status;
        }
    }

    async fn update_status_with_time(&self, id: &str, status: SessionStatus) {
        let mut map = self.info.write().await;
        if let Some(info) = map.get_mut(id) {
            info.status = status.clone();
            match status {
                SessionStatus::Running => {
                    if info.started_at.is_none() {
                        info.started_at = Some(chrono::Utc::now());
                    }
                }
                SessionStatus::Stopped | SessionStatus::Error(_) => {
                    if info.stopped_at.is_none() {
                        info.stopped_at = Some(chrono::Utc::now());
                    }
                }
                _ => {}
            }
        }
    }

    async fn get_status(&self, id: &str) -> Option<SessionStatus> {
        let map = self.info.read().await;
        map.get(id).map(|i| i.status.clone())
    }

    async fn filter_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values()
            .filter(|s| s.config_id == config_id)
            .cloned()
            .collect()
    }

    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .cloned()
            .collect()
    }
}

// ==================== Naming Service ====================

/// 会话命名服务 - 生成唯一的会话名称
pub trait NamingService: Send + Sync {
    fn generate_unique_name(
        &self,
        config_id: &str,
        base_name: &str,
        sessions: &[SessionInfo],
    ) -> String;
}

pub struct DefaultNamingService;

impl DefaultNamingService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultNamingService {
    fn default() -> Self {
        Self::new()
    }
}

impl NamingService for DefaultNamingService {
    fn generate_unique_name(
        &self,
        config_id: &str,
        base_name: &str,
        sessions: &[SessionInfo],
    ) -> String {
        let count = sessions
            .iter()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .count();

        if count == 0 {
            base_name.to_string()
        } else {
            format!("{}({})", base_name, count)
        }
    }
}

// ==================== Config Mapper ====================

/// 配置映射服务 - 将数据库配置转换为启动配置
pub trait ConfigMapper: Send + Sync {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig>;
}

pub struct DefaultConfigMapper;

impl DefaultConfigMapper {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultConfigMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigMapper for DefaultConfigMapper {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig> {
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
            env_vars: std::collections::HashMap::new(),
            cols: 120,
            rows: 40,
        })
    }
}

// ==================== Status Detector ====================

/// 状态检测服务 - 检测会话状态（如等待输入）
pub trait StatusDetector: Send + Sync {
    fn detect_waiting_input(&self, output: &str) -> bool;
}

pub struct DefaultStatusDetector;

impl DefaultStatusDetector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultStatusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusDetector for DefaultStatusDetector {
    fn detect_waiting_input(&self, output: &str) -> bool {
        crate::desktop::parser::detect_waiting_input(output)
    }
}
