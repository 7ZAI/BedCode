//! Session Configuration Management
//!
//! 会话配置管理模块 - 负责会话配置的创建、查询、修改、删除等操作
//! 提供配置的业务逻辑封装，与数据库层解耦

use crate::db::{Database, SessionConfig};
use crate::events::DesktopSyncEvent;
use crate::Result;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

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
        Self {
            db,
            sync_tx: RwLock::new(None),
        }
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
        let config = self
            .create_config_full_internal(name, environment, wsl_distro, working_dir, command, auto_start)
            .await?;

        // 发布同步事件：配置创建
        self.publish_sync_event(DesktopSyncEvent::ConfigCreated {
            config_id: config.id.clone(),
            source_device,
        })
        .await;

        Ok(config)
    }

    /// 创建新配置（内部实现）
    async fn create_config_full_internal(
        &self,
        name: String,
        environment: String,
        _wsl_distro: Option<String>,
        working_dir: String,
        command: String,
        _auto_start: bool,
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

        tracing::info!(config_id = %config_id, "Session config created: {}", config_name);
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
        self.create_config_with_source(name, environment, wsl_distro, working_dir, command, auto_start, None)
            .await
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
        self.update_config_with_source(
            config_id,
            name,
            environment,
            wsl_distro,
            working_dir,
            command,
            auto_start,
            None,
        )
        .await
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
        let existing = self
            .get_config(config_id)
            .await?
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
        })
        .await;

        tracing::info!(config_id = %config_id, "Session config updated: {}", updated_for_log);
        Ok(updated)
    }

    /// 删除配置
    pub async fn delete_config(&self, config_id: &str) -> Result<()> {
        self.delete_config_with_source(config_id, None).await
    }

    /// 删除配置（带来源设备）
    pub async fn delete_config_with_source(&self, config_id: &str, source_device: Option<String>) -> Result<()> {
        // 在删除前获取配置名称（用于同步通知）
        let config_name = self.get_config(config_id).await?.map(|c| c.name).unwrap_or_default();

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
        })
        .await;

        tracing::info!(config_id = %config_id, "Session config deleted");
        Ok(())
    }

    /// 验证配置参数
    pub fn validate_config(name: &str, environment: &str, _working_dir: &str, _command: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(crate::AppError::InvalidInput("Name cannot be empty".to_string()));
        }

        if environment.trim().is_empty() {
            return Err(crate::AppError::InvalidInput("Environment cannot be empty".to_string()));
        }

        // 验证环境类型（windows/wsl2/linux；前端 windows / wsl2 / linux 字面量，兼容 'powershell'/'cmd' 历史值）
        let valid_envs = ["powershell", "cmd", "wsl2", "windows", "linux"];
        let env_lower = environment.to_lowercase();
        if !valid_envs.iter().any(|e| env_lower.contains(e)) {
            tracing::warn!("Unknown environment type: {}", environment);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 构造独立内存库的管理器（测试隔离：不触碰全局单例）
    async fn manager() -> SessionConfigManager {
        let db = Arc::new(Mutex::new(Database::new(Path::new(":memory:")).expect("in-memory db")));
        db.lock().await.init_schema().expect("init schema");
        SessionConfigManager::new(db)
    }

    /// 创建一条配置并返回
    async fn create_one(m: &SessionConfigManager, name: &str) -> SessionConfig {
        m.create_config_full(
            name.to_string(),
            "linux".to_string(),
            None,
            "/home/u".to_string(),
            "bash".to_string(),
            false,
        )
        .await
        .expect("create config")
    }

    #[tokio::test]
    async fn create_config_returns_config_with_generated_id() {
        let m = manager().await;
        let cfg = create_one(&m, "dev").await;
        assert!(!cfg.id.is_empty(), "配置应生成非空 id");
        assert_eq!(cfg.name, "dev");
        assert_eq!(cfg.environment, "linux");
        assert_eq!(cfg.working_dir, "/home/u");
        // 落库可查
        let fetched = m.get_config(&cfg.id).await.unwrap().expect("config persisted");
        assert_eq!(fetched.id, cfg.id);
    }

    #[tokio::test]
    async fn create_config_full_internal_ignores_wsl_distro_and_auto_start() {
        // 锁定当前契约：_wsl_distro/_auto_start 是下划线前缀（有意丢弃）
        let m = manager().await;
        let cfg = m
            .create_config_with_source(
                "dev".to_string(),
                "linux".to_string(),
                Some("Ubuntu".to_string()),
                "/home/u".to_string(),
                "bash".to_string(),
                true,
                Some("d1".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(cfg.wsl_distro, None, "wsl_distro 参数被有意丢弃");
        assert!(!cfg.auto_start, "auto_start 参数被有意丢弃");
    }

    #[tokio::test]
    async fn get_config_missing_returns_none() {
        let m = manager().await;
        assert!(m.get_config("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_config_missing_returns_not_found() {
        let m = manager().await;
        let err = m
            .update_config("nonexistent", Some("x".into()), None, None, None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::AppError::NotFound(_)), "实际: {err}");
    }

    #[tokio::test]
    async fn update_config_partial_fields_preserves_existing() {
        let m = manager().await;
        let cfg = create_one(&m, "dev").await;
        // 只更新 name，其余字段保留
        let updated = m
            .update_config(&cfg.id, Some("renamed".into()), None, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.environment, "linux", "未指定字段应保留");
        assert_eq!(updated.working_dir, "/home/u");
        assert_eq!(updated.command, "bash");
        assert_eq!(updated.id, cfg.id, "id 不变");
        // 落库确认
        let fetched = m.get_config(&cfg.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "renamed");
    }

    #[tokio::test]
    async fn delete_config_removes_from_db() {
        let m = manager().await;
        let cfg = create_one(&m, "dev").await;
        m.delete_config(&cfg.id).await.unwrap();
        assert!(m.get_config(&cfg.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_config_missing_still_returns_ok() {
        // 锁定当前契约：删除不存在配置不报错
        let m = manager().await;
        assert!(m.delete_config("ghost").await.is_ok());
    }

    #[tokio::test]
    async fn delete_config_with_source_emits_config_removed_with_name() {
        let m = manager().await;
        let cfg = create_one(&m, "toBeDeleted").await;
        let (tx, mut rx) = tokio::sync::broadcast::channel::<DesktopSyncEvent>(8);
        m.set_sync_tx(tx).await;

        m.delete_config_with_source(&cfg.id, Some("d1".to_string()))
            .await
            .unwrap();
        let event = rx.recv().await.expect("应收到 ConfigRemoved 事件");
        match event {
            DesktopSyncEvent::ConfigRemoved {
                config_id,
                config_name,
                source_device,
            } => {
                assert_eq!(config_id, cfg.id);
                assert_eq!(config_name, "toBeDeleted", "事件应携带删除前读到的名字");
                assert_eq!(source_device.as_deref(), Some("d1"));
            }
            other => panic!("期望 ConfigRemoved，实际: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_config_with_sync_tx_emits_config_created() {
        let m = manager().await;
        let (tx, mut rx) = tokio::sync::broadcast::channel::<DesktopSyncEvent>(8);
        m.set_sync_tx(tx).await;

        m.create_config_with_source(
            "dev".to_string(),
            "linux".to_string(),
            None,
            "/home/u".to_string(),
            "bash".to_string(),
            false,
            Some("d2".to_string()),
        )
        .await
        .unwrap();
        let event = rx.recv().await.expect("应收到 ConfigCreated 事件");
        match event {
            DesktopSyncEvent::ConfigCreated {
                config_id,
                source_device,
            } => {
                assert!(!config_id.is_empty());
                assert_eq!(source_device.as_deref(), Some("d2"));
            }
            other => panic!("期望 ConfigCreated，实际: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_config_without_sync_tx_does_not_panic() {
        // sync_tx=None 时静默跳过（不 panic）
        let m = manager().await;
        let cfg = create_one(&m, "dev").await;
        assert!(!cfg.id.is_empty());
    }

    #[tokio::test]
    async fn update_config_emits_config_updated_with_source_device() {
        let m = manager().await;
        let cfg = create_one(&m, "dev").await;
        let (tx, mut rx) = tokio::sync::broadcast::channel::<DesktopSyncEvent>(8);
        m.set_sync_tx(tx).await;

        m.update_config_with_source(
            &cfg.id,
            Some("v2".into()),
            None,
            None,
            None,
            None,
            None,
            Some("d3".into()),
        )
        .await
        .unwrap();
        let event = rx.recv().await.expect("应收到 ConfigUpdated 事件");
        match event {
            DesktopSyncEvent::ConfigUpdated {
                config_id,
                source_device,
            } => {
                assert_eq!(config_id, cfg.id);
                assert_eq!(source_device.as_deref(), Some("d3"));
            }
            other => panic!("期望 ConfigUpdated，实际: {other:?}"),
        }
    }

    // ---- validate_config ----

    #[test]
    fn validate_rejects_empty_name() {
        assert!(SessionConfigManager::validate_config("", "linux", "/tmp", "bash").is_err());
        assert!(SessionConfigManager::validate_config("  ", "linux", "/tmp", "bash").is_err());
    }

    #[test]
    fn validate_rejects_empty_environment() {
        assert!(SessionConfigManager::validate_config("dev", "", "/tmp", "bash").is_err());
    }

    #[test]
    fn validate_accepts_all_known_environments() {
        for env in ["powershell", "cmd", "wsl2", "windows", "linux"] {
            assert!(
                SessionConfigManager::validate_config("dev", env, "/tmp", "bash").is_ok(),
                "应接受 {env}"
            );
        }
    }

    #[test]
    fn validate_allows_unknown_env_with_warning() {
        // 锁定当前宽松语义：未知环境仅 warn 不拒绝
        assert!(SessionConfigManager::validate_config("dev", "bogus-env", "/tmp", "bash").is_ok());
    }
}
