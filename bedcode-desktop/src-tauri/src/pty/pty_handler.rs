//! PTY Handler
//!
//! PTY 生命周期管理抽象 - 负责 PTY 会话的创建、运行、终止

use crate::pty::{PtySession, SessionLaunchConfig};
use crate::Result;

/// PTY 会话处理器 trait
///
/// 将 PTY 操作抽象为 trait，便于测试和替换实现
pub trait PtyHandler: Send + Sync {
    /// 创建新的 PTY 会话
    fn create_session(&self, config: SessionLaunchConfig) -> Result<PtySession>;

    /// 使用指定 ID 创建 PTY 会话（用于重启时复用旧 ID）
    fn create_session_with_id(&self, id: String, config: SessionLaunchConfig) -> Result<PtySession>;
}

/// PTY 会话处理器实现
///
/// 会话级运行状态由 `PtySession` 自身管理（`PtySession::is_running`）；
/// handler 只是会话工厂，不再携带应用级 running 标志（曾是无调用方的死字段，
/// 票据 07 删除）。
pub struct PtySessionHandler {}

impl PtySessionHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PtySessionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyHandler for PtySessionHandler {
    fn create_session(&self, config: SessionLaunchConfig) -> Result<PtySession> {
        PtySession::new(config)
    }

    fn create_session_with_id(&self, id: String, config: SessionLaunchConfig) -> Result<PtySession> {
        PtySession::with_id(id, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{ExecutionEnvironment, WindowsShell};
    use std::collections::HashMap;

    fn config() -> SessionLaunchConfig {
        SessionLaunchConfig {
            name: "test-session".to_string(),
            environment: ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
            working_dir: std::env::temp_dir().to_string_lossy().into_owned(),
            command: "echo hello".to_string(),
            env_vars: HashMap::new(),
            cols: 80,
            rows: 24,
        }
    }

    /// trait 真方法（生产调用点：session_manager 4 处）回归锁（票据 07）
    #[tokio::test]
    async fn create_session_returns_session_with_config() {
        let handler = PtySessionHandler::new();
        let session = handler.create_session(config()).expect("create_session");
        assert_eq!(session.name().await, "test-session");
        assert!(!session.id().is_empty());
    }

    #[tokio::test]
    async fn create_session_with_id_uses_requested_id() {
        let handler = PtySessionHandler::new();
        let session = handler
            .create_session_with_id("restart-123".to_string(), config())
            .expect("create_session_with_id");
        assert_eq!(session.id(), "restart-123", "指定 ID 必须透传（重启复用旧 ID）");
        assert_eq!(session.name().await, "test-session");
    }

    #[test]
    fn default_equals_new() {
        let _ = PtySessionHandler::default();
        let _ = PtySessionHandler::new();
    }
}
