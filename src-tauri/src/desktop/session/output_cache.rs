//! Output Cache
//!
//! PTY 输出缓存 - 为移动端订阅提供历史输出

use crate::desktop::pty::PtyOutputEvent;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// 输出缓存 trait
pub trait OutputCache: Send + Sync {
    async fn cache(&self, event: PtyOutputEvent);
    async fn get(&self, session_id: &str) -> Vec<PtyOutputEvent>;
    async fn clear(&self, session_id: &str);
    async fn clear_all(&self);
    async fn len(&self) -> usize;
}

/// 输出缓存实现
pub struct DefaultOutputCache {
    cache: Arc<RwLock<HashMap<String, Vec<PtyOutputEvent>>>>,
    max_size: usize,
}

impl DefaultOutputCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }
}

impl OutputCache for DefaultOutputCache {
    async fn cache(&self, event: PtyOutputEvent) {
        let mut cache = self.cache.write().await;
        let entries = cache.entry(event.session_id.clone()).or_insert_with(Vec::new);

        if entries.len() >= self.max_size {
            entries.remove(0);
        }
        entries.push(event.clone());
    }

    async fn get(&self, session_id: &str) -> Vec<PtyOutputEvent> {
        let cache = self.cache.read().await;
        cache.get(session_id).cloned().unwrap_or_default()
    }

    async fn clear(&self, session_id: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(session_id);
    }

    async fn clear_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.values().map(Vec::len).sum()
    }
}