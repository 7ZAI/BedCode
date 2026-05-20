//! Session Configuration Management
//!
//! 会话配置管理模块 - 负责会话配置的创建、查询、修改、删除等操作
//! 提供配置的业务逻辑封装，与数据库层解耦

use crate::shared::db::{Database, SessionConfig};
use crate::Result;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 会话配置管理器
///
/// 职责：
/// - 配置的创建、查询、修改、删除
/// - 配置验证
/// - 配置默认值处理
/// - 与数据库层交互
pub struct SessionConfigManager {
    db: Arc<Mutex<Database>>,
}

impl SessionConfigManager {
    /// 创建新的配置管理器
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// 从 Database 创建（兼容旧 API）
    pub fn from_database(db: Database) -> Self {
        Self::new(Arc::new(Mutex::new(db)))
    }

    /// 创建新配置
    pub async fn create_config(
        &self,
        name: String,
        environment: String,
        working_dir: String,
        command: String,
    ) -> Result<SessionConfig> {
        let config = SessionConfig::new(name, environment, working_dir, command);
        let config_id = config.id.clone();
        let config_name = config.name.clone();
        let result_config = config.clone();

        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.create_session_config(&config)
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))??;

        tracing::info!("Session config created: {} ({})", config_name, config_id);
        Ok(result_config)
    }

    /// 创建新配置（带完整参数）
    pub async fn create_config_full(
        &self,
        name: String,
        environment: String,
        wsl_distro: Option<String>,
        working_dir: String,
        command: String,
        tmux_session: Option<String>,
        auto_start: bool,
    ) -> Result<SessionConfig> {
        tracing::info!("SessionConfigManager::create_config_full called: name={}", name);

        let now = Utc::now();
        let config = SessionConfig {
            id: Uuid::new_v4().to_string(),
            name,
            environment,
            wsl_distro,
            working_dir,
            command,
            tmux_session,
            auto_start,
            created_at: now,
            updated_at: now,
        };
        let config_id = config.id.clone();
        let config_name = config.name.clone();
        let result_config = config.clone();

        tracing::info!("Config created in memory: id={}", config_id);

        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            tracing::info!("spawn_blocking: starting db insert");
            let db = db.blocking_lock();
            let insert_result = db.create_session_config(&config);
            tracing::info!("spawn_blocking: db insert completed");
            insert_result
        })
        .await;

        tracing::info!("spawn_blocking result received");

        match result {
            Ok(Ok(())) => {
                tracing::info!("Session config created: {} ({})", config_name, config_id);
                Ok(result_config)
            }
            Ok(Err(e)) => {
                tracing::error!("Database error: {:?}", e);
                Err(e)
            }
            Err(e) => {
                tracing::error!("Task join error: {:?}", e);
                Err(crate::AppError::Internal(format!("Task join error: {}", e)))
            }
        }
    }

    /// 获取配置
    pub async fn get_config(&self, config_id: &str) -> Result<Option<SessionConfig>> {
        let db = self.db.clone();
        let config_id = config_id.to_string();

        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.get_session_config(&config_id)
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))?
    }

    /// 获取所有配置
    pub async fn list_configs(&self) -> Result<Vec<SessionConfig>> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.get_session_configs()
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))?
    }

    /// 更新配置
    pub async fn update_config(
        &self,
        config_id: &str,
        name: Option<String>,
        environment: Option<String>,
        wsl_distro: Option<String>,
        working_dir: Option<String>,
        command: Option<String>,
        tmux_session: Option<String>,
        auto_start: Option<bool>,
    ) -> Result<SessionConfig> {
        // 先获取现有配置
        let existing = self.get_config(config_id).await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        let updated = SessionConfig {
            id: existing.id,
            name: name.unwrap_or(existing.name),
            environment: environment.unwrap_or(existing.environment),
            wsl_distro: wsl_distro.or(existing.wsl_distro),
            working_dir: working_dir.unwrap_or(existing.working_dir),
            command: command.unwrap_or(existing.command),
            tmux_session: tmux_session.or(existing.tmux_session),
            auto_start: auto_start.unwrap_or(existing.auto_start),
            created_at: existing.created_at,
            updated_at: Utc::now(),
        };

        let db = self.db.clone();
        let config_id = config_id.to_string();
        let updated_for_log = updated.name.clone();
        let updated_for_db = updated.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.update_session_config(&updated_for_db)
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))??;

        tracing::info!("Session config updated: {} ({})", updated_for_log, config_id);
        Ok(updated)
    }

    /// 删除配置
    pub async fn delete_config(&self, config_id: &str) -> Result<()> {
        let db = self.db.clone();
        let config_id_owned = config_id.to_string();

        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.delete_session_config(&config_id_owned)
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))??;

        tracing::info!("Session config deleted: {}", config_id);
        Ok(())
    }

    /// 验证配置参数
    pub fn validate_config(
        name: &str,
        environment: &str,
        _working_dir: &str,
        _command: &str,
    ) -> Result<()> {
        if name.trim().is_empty() {
            return Err(crate::AppError::InvalidInput("Name cannot be empty".to_string()));
        }

        if environment.trim().is_empty() {
            return Err(crate::AppError::InvalidInput("Environment cannot be empty".to_string()));
        }

        // 验证环境类型
        let valid_envs = ["powershell", "cmd", "wsl2"];
        let env_lower = environment.to_lowercase();
        if !valid_envs.iter().any(|e| env_lower.contains(e)) {
            tracing::warn!("Unknown environment type: {}", environment);
        }

        Ok(())
    }
}