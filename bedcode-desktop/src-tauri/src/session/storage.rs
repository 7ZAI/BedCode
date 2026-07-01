//! Session Storage
//!
//! 会话存储抽象 - 负责会话配置的数据库操作

use crate::db::Database;
use crate::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 会话配置查询 trait
///
/// 将数据库操作抽象为 trait，便于测试和替换实现
pub trait SessionStore: Send + Sync {
    /// 获取会话配置（异步）
    async fn get_config(&self, config_id: &str) -> Result<Option<crate::db::SessionConfig>>;

    /// 获取所有会话配置（异步）
    async fn list_configs(&self) -> Result<Vec<crate::db::SessionConfig>>;

    /// 删除会话配置（异步）
    async fn delete_config(&self, config_id: &str) -> Result<()>;
}

/// 会话配置存储实现
pub struct SessionStorage {
    db: Arc<Mutex<Database>>,
}

impl SessionStorage {
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }
}

impl SessionStore for SessionStorage {
    async fn get_config(&self, config_id: &str) -> Result<Option<crate::db::SessionConfig>> {
        let db = self.db.clone();
        let config_id = config_id.to_string();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.get_session_config(&config_id)
        }).await.map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))?
    }

    async fn list_configs(&self) -> Result<Vec<crate::db::SessionConfig>> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.get_session_configs()
        }).await.map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))?
    }

    async fn delete_config(&self, config_id: &str) -> Result<()> {
        let db = self.db.clone();
        let config_id = config_id.to_string();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.delete_session_config(&config_id)
        }).await.map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))?
    }
}