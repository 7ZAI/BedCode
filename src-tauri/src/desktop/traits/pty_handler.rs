//! PTY Handler Trait
//!
//! PTY 处理器 trait 定义

use crate::desktop::pty::{PtySession, SessionLaunchConfig};
use crate::Result;

pub trait PtyHandler: Send + Sync {
    fn create_session(&self, config: SessionLaunchConfig) -> Result<PtySession>;
    fn create_session_with_id(&self, id: String, config: SessionLaunchConfig) -> Result<PtySession>;
}