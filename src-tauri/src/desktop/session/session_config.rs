//! Session Configuration Management
//!
//! 会话配置管理模块 - 负责会话配置的创建、查询、修改、删除等操作
//! 提供配置的业务逻辑封装，与数据库层解耦

use crate::desktop::events::DesktopSyncEvent;
use crate::shared::db::{Database, SessionConfig};
use crate::Result;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

/// 会话配置管理器
///
/// 职责：
/// - 配置的创建、查询、修改、删除
/// - 配置验证
/// - 配置默认值处理
/// - 与数据库层交互
/// - 发布同步事件（向客户端广播增量数据）
pub struct SessionConfigManager {
    db: Arc<Mutex<Database>>,
    /// 同步事件发送器（用于向客户端广播增量数据）
    sync_tx: RwLock<Option<broadcast::Sender<DesktopSyncEvent>>>,
}

impl SessionConfigManager {
    /// 创建新的配置管理器
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db, sync_tx: RwLock::new(None) }
    }

    /// 从 Database 创建（兼容旧 API）
    pub fn from_database(db: Database) -> Self {
        Self::new(Arc::new(Mutex::new(db)))
    }

    /// 设置同步事件发送器
    pub async fn set_sync_tx(&self, sync_tx: broadcast::Sender<DesktopSyncEvent>) {
        let mut tx = self.sync_tx.write().await;
        *tx = Some(sync_tx);
    }

    /// 发布同步事件
    async fn publish_sync_event(&self, event: DesktopSyncEvent) {
        let tx = self.sync_tx.read().await;
        if let Some(sender) = &*tx {
            let _ = sender.send(event);
        }
    }

    /// 创建新配置
    pub async fn create_config(
        &self,
        name: String,
        environment: String,
        working_dir: String,
        command: String,
    ) -> Result<SessionConfig> {
        self.create_config_with_source(name, environment, None, working_dir, command, false, None).await
    }

    /// 创建新配置（带来源设备）
    pub async fn create_config_with_source(
        &self,
        name: String,
        environment: String,
        wsl_distro: Option<String>,
        working_dir: String,
        command: String,
        auto_start: bool,
        source_device: Option<String>,
    ) -> Result<SessionConfig> {
        let config = self.create_config_full_internal(
            name,
            environment,
            wsl_distro,
            working_dir,
            command,
            auto_start,
        ).await?;

        // 发布同步事件：配置创建
        self.publish_sync_event(DesktopSyncEvent::ConfigCreated {
            config_id: config.id.clone(),
            source_device,
        }).await;

        Ok(config)
    }

    /// 创建新配置（内部实现）
    async fn create_config_full_internal(
        &self,
        name: String,
        environment: String,
        wsl_distro: Option<String>,
        working_dir: String,
        command: String,
        auto_start: bool,
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
        auto_start: bool,
    ) -> Result<SessionConfig> {
        self.create_config_with_source(name, environment, wsl_distro, working_dir, command, auto_start, None).await
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
        auto_start: Option<bool>,
    ) -> Result<SessionConfig> {
        self.update_config_with_source(config_id, name, environment, wsl_distro, working_dir, command, auto_start, None).await
    }

    /// 更新配置（带来源设备）
    pub async fn update_config_with_source(
        &self,
        config_id: &str,
        name: Option<String>,
        environment: Option<String>,
        wsl_distro: Option<String>,
        working_dir: Option<String>,
        command: Option<String>,
        auto_start: Option<bool>,
        source_device: Option<String>,
    ) -> Result<SessionConfig> {
        // 先获取现有配置
        let existing = self.get_config(config_id).await?
            .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", config_id)))?;

        let updated = SessionConfig {
            id: existing.id.clone(),
            name: name.unwrap_or(existing.name),
            environment: environment.unwrap_or(existing.environment),
            wsl_distro: wsl_distro.or(existing.wsl_distro),
            working_dir: working_dir.unwrap_or(existing.working_dir),
            command: command.unwrap_or(existing.command),
            auto_start: auto_start.unwrap_or(existing.auto_start),
            created_at: existing.created_at,
            updated_at: Utc::now(),
        };

        let db = self.db.clone();
        let config_id_owned = config_id.to_string();
        let updated_for_log = updated.name.clone();
        let updated_for_db = updated.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.update_session_config(&updated_for_db)
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))??;

        // 发布同步事件：配置更新
        self.publish_sync_event(DesktopSyncEvent::ConfigUpdated {
            config_id: config_id_owned,
            source_device,
        }).await;

        tracing::info!("Session config updated: {} ({})", updated_for_log, config_id);
        Ok(updated)
    }

    /// 删除配置
    pub async fn delete_config(&self, config_id: &str) -> Result<()> {
        self.delete_config_with_source(config_id, None).await
    }

    /// 删除配置（带来源设备）
    pub async fn delete_config_with_source(&self, config_id: &str, source_device: Option<String>) -> Result<()> {
        // 在删除前获取配置名称（用于同步通知）
        let config_name = self.get_config(config_id).await?
            .map(|c| c.name)
            .unwrap_or_default();

        let db = self.db.clone();
        let config_id_owned = config_id.to_string();

        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.delete_session_config(&config_id_owned)
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))??;

        // 发布同步事件：配置删除
        self.publish_sync_event(DesktopSyncEvent::ConfigRemoved {
            config_id: config_id.to_string(),
            config_name,
            source_device,
        }).await;

        tracing::info!("Session config deleted: {}", config_id);
        Ok(())
    }

    /// 根据 session_id 获取会话配置
    ///
    /// 先通过 SessionManager 查找 SessionInfo 获取 config_id，
    /// 再根据 config_id 加载完整配置
    pub async fn get_config_by_session_id(
        &self,
        session_id: &str,
        session_manager: &crate::desktop::session::SessionManager,
    ) -> Result<SessionConfig> {
        let info = session_manager.get_session_info(session_id).await?;
        self.get_config(&info.config_id).await?
            .ok_or_else(|| crate::AppError::NotFound(format!(
                "Config not found: {} (session: {})", info.config_id, session_id
            )))
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