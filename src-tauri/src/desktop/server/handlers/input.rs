//! Input Handler
//!
//! 输入消息处理 - 将移动端输入转发到 PTY 或 Plugin 会话

use crate::desktop::plugin::PluginManager;
use crate::desktop::server::message::SpecialKey;
use crate::desktop::session::SessionManager;
use crate::desktop::session::SessionType;
use crate::Result;
use std::sync::Arc;

/// 处理输入消息
pub async fn handle_input(
    session_id: &str,
    data: &str,
    special_key: Option<SpecialKey>,
    session_manager: &Arc<SessionManager>,
    plugin_manager: &Arc<PluginManager>,
) -> Result<()> {
    // 检查会话类型
    let is_plugin = session_manager
        .get_session(session_id)
        .await
        .map(|s| s.session_type == SessionType::Plugin)
        .unwrap_or(false);

    if is_plugin {
        plugin_manager.write_input(session_id, data).await?;
    } else {
        if let Some(key) = special_key {
            session_manager.send_special_key(session_id, key.as_str()).await?;
        } else {
            session_manager.write_input(session_id, data).await?;
        }
    }
    Ok(())
}