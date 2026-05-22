//! Session Config Service
//!
//! 会话配置 CRUD 服务

use crate::desktop::server::message::{ControlAction, Message, SessionConfigSummary, QuickActionSummary};
use crate::shared::db::Database;
use crate::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 获取会话配置列表并构建响应消息
pub async fn list_session_configs(
    request_message_id: String,
    db: &Arc<Mutex<Database>>,
) -> Result<Option<Message>> {
    let db = db.lock().await;
    let configs = db.get_session_configs()?;
    drop(db);

    let summaries: Vec<SessionConfigSummary> = configs
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

    Ok(Some(Message::Control {
        message_id: request_message_id,
        session_id: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
        payload: crate::desktop::server::message::ControlPayload {
            action: ControlAction::SessionConfigList { configs: summaries },
        },
    }))
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