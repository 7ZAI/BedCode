//! Session Config Service
//!
//! 会话配置 CRUD 服务

use crate::desktop::session::SessionConfigManager;
use crate::desktop::server::message::{SessionConfigSummary, QuickActionSummary};
use crate::shared::db::Database;
use crate::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 获取会话配置列表
pub async fn list_session_configs(
    config_manager: &SessionConfigManager,
) -> Result<Vec<SessionConfigSummary>> {
    let configs = config_manager.list_configs().await?;

    let summaries = configs
        .into_iter()
        .map(|c| SessionConfigSummary {
            id: c.id,
            name: c.name,
            environment: c.environment,
            wsl_distro: c.wsl_distro,
            working_dir: c.working_dir,
            command: c.command,
        })
        .collect();

    Ok(summaries)
}

/// 获取快捷指令列表
pub async fn list_quick_actions(db: &Arc<Mutex<Database>>) -> Result<Vec<QuickActionSummary>> {
    let db = db.lock().await;
    let actions = db.get_quick_actions()?;
    drop(db);

    let summaries = actions
        .into_iter()
        .map(|a| QuickActionSummary {
            id: a.id,
            name: a.name,
            content: a.content,
            icon: a.icon,
            color: a.color,
        })
        .collect();

    Ok(summaries)
}