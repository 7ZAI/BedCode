//! PTY Registry
//!
//! PTY 会话注册表 - 负责 PTY 会话的存储和基本操作

use crate::desktop::pty::PtySession;
use crate::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// PTY 会话注册表 trait
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

/// PTY 会话注册表实现
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