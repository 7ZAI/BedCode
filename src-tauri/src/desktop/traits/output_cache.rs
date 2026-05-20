//! Output Cache Trait
//!
//! 输出缓存 trait 定义

use crate::desktop::pty::PtyOutputEvent;

pub trait OutputCache: Send + Sync {
    async fn cache(&self, event: PtyOutputEvent);
    async fn get(&self, session_id: &str) -> Vec<PtyOutputEvent>;
    async fn clear(&self, session_id: &str);
    async fn clear_all(&self);
    async fn len(&self) -> usize;
}