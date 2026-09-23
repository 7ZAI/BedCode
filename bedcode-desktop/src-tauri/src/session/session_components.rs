//! Session Components
//!
//! 会话管理器的内部组件：PTY / 会话信息 / 正统渲染端注册表
//! 这些组件各自只有一个实现，trait 已内联到此文件
//!
//! v21 起「命名服务 / 配置映射」已退役（映射决策归插件）：本文件不再含
//! NamingService / ConfigMapper——插件侧的对应实现见
//! `plugins/terminal-session/rust/src/launch.rs`。

use crate::enums::SessionStatus;
use crate::protocol::{RendererSource, SessionInfo};
use crate::pty::PtySession;
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
                tracing::error!(session_id = %id, error = %e, "Failed to kill session");
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
    /// 改名（票 10）：返回改名前的名字；会话不存在返回 `None`（调用方显性报错）
    async fn rename(&self, id: &str, name: &str) -> Option<String>;
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
        map.values().filter(|s| s.config_id == config_id).cloned().collect()
    }

    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .cloned()
            .collect()
    }

    async fn rename(&self, id: &str, name: &str) -> Option<String> {
        let mut map = self.info.write().await;
        let info = map.get_mut(id)?;
        let previous = info.name.clone();
        info.name = name.to_string();
        Some(previous)
    }
}

// ==================== Canonical Renderer Registry ====================
//
// `RendererSource` / `ResizeOutcome` 是对外 wire 形状，已迁 `crate::protocol::session`
// （票 02）；本目录只留「当前归属端」这份登记事实的存取实现。

/// 启动初始网格解析：启动端携带且合法（>0）时覆盖配置默认尺寸
///
/// 各端终端组件按自身窗口/字体预算出默认网格随启动请求传入，PTY openpty
/// 直接以该尺寸创建，避免「先 80x24 启动 → 挂载后再 resize」的首帧回绕。
pub fn resolve_initial_size(base_cols: u16, base_rows: u16, initial: Option<(u16, u16)>) -> (u16, u16) {
    match initial {
        Some((cols, rows)) if cols > 0 && rows > 0 => (cols, rows),
        _ => (base_cols, base_rows),
    }
}

/// 正统渲染端注册表 - 每会话记录当前 PTY 尺寸归属端
pub trait CanonicalRendererRegistry: Send + Sync {
    async fn get(&self, session_id: &str) -> Option<RendererSource>;
    async fn set(&self, session_id: &str, source: RendererSource);
    async fn clear(&self, session_id: &str);
}

pub struct DefaultCanonicalRendererRegistry {
    map: Arc<RwLock<HashMap<String, RendererSource>>>,
}

impl DefaultCanonicalRendererRegistry {
    pub fn new() -> Self {
        Self {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultCanonicalRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CanonicalRendererRegistry for DefaultCanonicalRendererRegistry {
    async fn get(&self, session_id: &str) -> Option<RendererSource> {
        let map = self.map.read().await;
        map.get(session_id).cloned()
    }

    async fn set(&self, session_id: &str, source: RendererSource) {
        let mut map = self.map.write().await;
        map.insert(session_id.to_string(), source);
    }

    async fn clear(&self, session_id: &str) {
        let mut map = self.map.write().await;
        map.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 启动初始网格解析：合法尺寸覆盖默认值，非法（0）或缺省回退配置默认
    #[test]
    fn test_resolve_initial_size_overrides_only_when_valid() {
        assert_eq!(resolve_initial_size(80, 24, Some((120, 40))), (120, 40));
        assert_eq!(resolve_initial_size(80, 24, None), (80, 24));
        // 0 尺寸（隐藏容器误传）不生效
        assert_eq!(resolve_initial_size(80, 24, Some((0, 40))), (80, 24));
        assert_eq!(resolve_initial_size(80, 24, Some((120, 0))), (80, 24));
    }
}
