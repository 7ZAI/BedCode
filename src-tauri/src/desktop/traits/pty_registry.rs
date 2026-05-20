//! PTY Registry Trait
//!
//! PTY 会话注册表 trait 定义

use crate::desktop::pty::PtySession;
use crate::Result;

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