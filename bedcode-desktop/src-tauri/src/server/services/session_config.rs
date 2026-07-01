//! Session Config Service
//!
//! 会话配置 CRUD 服务

use crate::server::message::{SessionConfigAction, SessionConfigPayload, Message, SessionConfigSummary, QuickActionSummary};
use crate::session::SessionConfigManager;
use crate::db::Database;
use crate::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 获取会话配置列表并构建响应消息
pub async fn list_session_configs(
    request_message_id: String,
    db: &Arc<Mutex<Database>>,
) -> Result<Option<Message>> {
    let manager = SessionConfigManager::new(db.clone());
    let configs = manager.list_configs().await?;

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

    Ok(Some(Message::SessionConfig {
        message_id: request_message_id,
        expect_response: false,
        session_id: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
        token: String::new(),
        payload: SessionConfigPayload {
            action: SessionConfigAction::SessionConfigList { configs: summaries },
        },
    }))
}

/// 获取快捷指令列表并构建响应消息
pub async fn list_quick_actions_response(request_message_id: String) -> Result<Option<Message>> {
    let actions = list_quick_actions().await?;

    Ok(Some(Message::SessionConfig {
        message_id: request_message_id,
        expect_response: false,
        session_id: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
        token: String::new(),
        payload: SessionConfigPayload {
            action: SessionConfigAction::QuickActionList { actions },
        },
    }))
}

/// 获取快捷指令列表
///
/// TODO(binblink): 实现从 SessionConfigManager 获取快捷指令列表
/// 当前返回空列表，待接入数据库或配置存储后实现真正的业务逻辑
pub async fn list_quick_actions() -> Result<Vec<QuickActionSummary>> {
    // TODO: 实现快捷指令列表查询
    // 暂时返回空列表，后续需要从数据库或配置文件中加载
    Ok(vec![])
}