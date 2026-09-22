//! Session Config Read Access（legacy 迁移通道，v22 起只读）
//!
//! 业务配置真源在 `com.bedcode.terminal-session` 插件私有库（票 08）；宿主
//! `SessionConfigManager` 收缩为**只读面**，唯一使命是给插件的**一次性 legacy 迁移**
//! 提供主库 `session_configs` 表访问（老版本升级残留，插件 `config/ops.rs::migrate`
//! 经 host-session 读取面迁入私有库，marker 幂等）。
//!
//! **写面（create/update/delete + DesktopSyncEvent 发布）随 v22 退役**：
//! - host-session 配置面写原语（`config-upsert` / `config-delete`）无调用者——插件写
//!   路径自票 08 起全走私有库，宿主写原语是死接口，删除不改变任何行为
//! - 宿主命令面与桥接投影（`upsert_config` / `session_config_bridge`）已随票 05 注销
//! - 引擎层 SQL 写接口（`crate::db::*SessionConfig*`）按 §9 「SQLite 引擎层」保留为
//!   基础服务能力；**业务层不再暴露写通道**（本管理器不再提供，WIT 不再导入）
//!
//! 读取面由 `host_impl/session.rs` 的 `session_config_list` / `session_config_get`
//! 承接（权限 `session:read`）；迁移窗口结束（主库表退役）后随表一并删除。

use crate::db::{Database, SessionConfig};
use crate::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 会话配置只读管理器（legacy 迁移通道）
pub struct SessionConfigManager {
    db: Arc<Mutex<Database>>,
}

impl SessionConfigManager {
    /// 创建只读配置管理器
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// 获取配置（不存在返回 None）
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

    /// 获取所有配置（db 层按 name 排序）
    pub async fn list_configs(&self) -> Result<Vec<SessionConfig>> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.get_session_configs()
        })
        .await
        .map_err(|e| crate::AppError::Internal(format!("Task join error: {}", e)))?
    }

    /// 所持引擎库句柄（crate 内可见）
    ///
    /// 仅测试播种 legacy 行用（e2e 模拟老版本主库残留经本管理器读面验证迁移）；
    /// 生产路径不传它——写接口只在 `crate::db` 引擎层，业务层无写通道。
    pub(crate) fn db(&self) -> Arc<Mutex<Database>> {
        Arc::clone(&self.db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 构造独立内存库的只读管理器（测试隔离：不触碰全局单例）
    ///
    /// 播种走引擎层 SQL 写接口（`db.create_session_config`，模拟老版本升级残留
    /// 直接从主库表读出）——本管理器自身**不提供写**，这正是 v22 收缩的契约。
    async fn manager() -> SessionConfigManager {
        let db = Arc::new(Mutex::new(Database::new(Path::new(":memory:")).expect("in-memory db")));
        db.lock().await.init_schema().expect("init schema");
        SessionConfigManager::new(db)
    }

    /// 引擎层直插一条 legacy 行，返回落库后的配置
    async fn seed_one(
        db: &Arc<Mutex<Database>>,
        name: &str,
        environment: &str,
        working_dir: &str,
        command: &str,
    ) -> SessionConfig {
        let config = SessionConfig::new(name.to_string(), environment.to_string(), working_dir.to_string(), command.to_string());
        let db = db.clone();
        let cfg = config.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.create_session_config(&cfg)
        })
        .await
        .expect("seed task join")
        .expect("seed legacy row");
        config
    }

    #[tokio::test]
    async fn get_config_missing_returns_none() {
        let m = manager().await;
        assert!(m.get_config("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_config_empty_id_returns_none() {
        // 锁定只读面边界：空 id 不报错、不扫描（db 层按主键查，无匹配即 None）
        let m = manager().await;
        assert!(m.get_config("").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_config_returns_seeded_row() {
        let db = Arc::new(Mutex::new(Database::new(Path::new(":memory:")).expect("in-memory db")));
        db.lock().await.init_schema().expect("init schema");
        let seeded = seed_one(&db, "dev", "linux", "/home/u", "bash").await;

        let m = SessionConfigManager::new(db);
        let fetched = m.get_config(&seeded.id).await.unwrap().expect("读到播种行");
        assert_eq!(fetched.id, seeded.id);
        assert_eq!(fetched.name, "dev");
        assert_eq!(fetched.environment, "linux");
        assert_eq!(fetched.working_dir, "/home/u");
        assert_eq!(fetched.command, "bash");
        assert!(!fetched.auto_start, "auto_start 缺省 false");
    }

    #[tokio::test]
    async fn list_configs_returns_all_ordered_by_name() {
        let db = Arc::new(Mutex::new(Database::new(Path::new(":memory:")).expect("in-memory db")));
        db.lock().await.init_schema().expect("init schema");
        seed_one(&db, "Beta", "linux", "/a", "bash").await;
        seed_one(&db, "Alpha", "wsl2", "/b", "cmd").await;

        let m = SessionConfigManager::new(db);
        let all = m.list_configs().await.unwrap();
        assert_eq!(all.len(), 2, "两条播种行都应列出");
        // 迁移通道拿到的 id 清单顺序无关紧要（插件侧按 id 逐条 get），
        // 但 db 层 ORDER BY name 保证确定性——锁定该排序
        assert_eq!(all[0].name, "Alpha");
        assert_eq!(all[1].name, "Beta");
    }

    #[tokio::test]
    async fn list_configs_empty_db_returns_empty() {
        let m = manager().await;
        assert!(m.list_configs().await.unwrap().is_empty());
    }
}