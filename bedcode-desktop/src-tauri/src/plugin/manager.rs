//! Plugin Task Status Manager
//!
//! 管理插件推送的任务执行状态，并通过事件总线广播变更。
//! 同时管理会话级自动授权模式，供 Python PreToolUse hook 通过 HTTP API 查询。
//!
//! 插件（bedcode-plugin）通过 HTTP API 推送任务状态变更，
//! 本模块负责接收状态、内存存储、以及通过 DesktopSyncEvent 广播到所有客户端。
//!
//! 自动授权模式：移动端/终端切换自动模式时，通过 HTTP API 通知桌面端，
//! PluginManager 在内存中维护模式状态，Python hook 通过 GET /api/plugin/session-mode 查询。

use crate::enums::PluginQuestion;
use crate::enums::TaskStatus;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 任务状态条目
#[derive(Debug, Clone)]
struct TaskStateEntry {
    task_status: TaskStatus,
    task_reason: Option<String>,
    task_questions: Option<Vec<PluginQuestion>>,
    updated_at: DateTime<Utc>,
}

/// 插件任务状态管理器
///
/// 管理插件推送的任务执行状态和会话级自动授权模式。
/// 自动授权模式仅存储在内存中，Python hook 通过 HTTP API 查询。
pub struct PluginManager {
    task_states: Arc<RwLock<HashMap<String, TaskStateEntry>>>,
    /// 会话级自动授权模式：bedcode_session_id → auto_approve
    auto_modes: Arc<RwLock<HashMap<String, bool>>>,
    /// Claude Code session_id → BedCode PTY session_id 映射
    session_id_map: Arc<RwLock<HashMap<String, String>>>,
}

impl PluginManager {
    /// 创建新的 PluginManager
    pub fn new() -> Self {
        Self {
            task_states: Arc::new(RwLock::new(HashMap::new())),
            auto_modes: Arc::new(RwLock::new(HashMap::new())),
            session_id_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl PluginManager {
    /// 注册 Claude Code session_id → BedCode PTY session_id 映射
    ///
    /// 由 SessionStart hook 通过 HTTP API 调用，携带 bedcode_session_id 时自动注册。
    pub async fn register_session_mapping(&self, claude_session_id: &str, bedcode_session_id: &str) {
        let mut map = self.session_id_map.write().await;
        map.insert(claude_session_id.to_string(), bedcode_session_id.to_string());
        tracing::info!(
            "Session mapping registered: claude_sid={} → bedcode_sid={}",
            claude_session_id,
            bedcode_session_id
        );
    }

    /// 将 Claude Code session_id 解析为 BedCode PTY session_id
    ///
    /// 如果存在映射则返回 BedCode session ID，否则返回原始值。
    pub async fn resolve_session_id(&self, claude_session_id: &str) -> String {
        let map = self.session_id_map.read().await;
        match map.get(claude_session_id) {
            Some(bedcode_id) => bedcode_id.clone(),
            None => claude_session_id.to_string(),
        }
    }

    /// 更新插件推送的任务状态
    ///
    /// 由 HTTP API `POST /api/plugin/task-status` 调用。
    /// 更新内存中的任务状态，并通过 DesktopSyncEvent 广播变更到所有 WebSocket 客户端。
    pub async fn update_task_status(
        &self,
        session_id: &str,
        task_status: TaskStatus,
        task_reason: Option<String>,
        task_questions: Option<Vec<PluginQuestion>>,
    ) -> Result<(), crate::AppError> {
        // 更新内存状态
        {
            let mut states = self.task_states.write().await;
            states.insert(
                session_id.to_string(),
                TaskStateEntry {
                    task_status: task_status.clone(),
                    task_reason: task_reason.clone(),
                    task_questions: task_questions.clone(),
                    updated_at: Utc::now(),
                },
            );
        }

        // 通过 AppContext 获取 sync_tx 广播 DesktopSyncEvent
        {
            use crate::events::DesktopSyncEvent;
            let ctx = crate::system::app_context::AppContext::global();
            let sync_tx = ctx.sync_tx();
            let event = DesktopSyncEvent::TaskStatusChanged {
                session_id: session_id.to_string(),
                // 使用 serde 序列化确保输出 snake_case（如 "in_progress"），
                // 而非 Debug 格式（如 "InProgress" -> to_lowercase -> "inprogress"）
                task_status: serde_json::to_string(&task_status)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string(),
                task_reason,
                task_questions,
            };
            let _ = sync_tx.send(event);
        }

        Ok(())
    }

    /// 获取指定会话的任务状态
    pub async fn get_task_status(&self, session_id: &str) -> Option<TaskStatus> {
        let states = self.task_states.read().await;
        states.get(session_id).map(|e| e.task_status.clone())
    }

    /// 获取指定会话的任务状态原因
    pub async fn get_task_reason(&self, session_id: &str) -> Option<String> {
        let states = self.task_states.read().await;
        states.get(session_id).and_then(|e| e.task_reason.clone())
    }

    /// 移除指定会话的任务状态
    pub async fn remove_task_status(&self, session_id: &str) {
        let mut states = self.task_states.write().await;
        states.remove(session_id);
    }

    // ==================== 会话自动授权模式 ====================

    /// 设置会话自动授权模式
    ///
    /// 由 HTTP API `POST /api/plugin/session-mode` 调用。
    /// 更新内存状态，并通过 DesktopSyncEvent 广播变更到所有 WebSocket 客户端。
    pub async fn set_auto_mode(&self, session_id: &str, auto_approve: bool) {
        {
            let mut modes = self.auto_modes.write().await;
            modes.insert(session_id.to_string(), auto_approve);
        }

        // 广播模式变更
        {
            use crate::events::DesktopSyncEvent;
            let ctx = crate::system::app_context::AppContext::global();
            let sync_tx = ctx.sync_tx();
            let event = DesktopSyncEvent::SessionModeChanged {
                session_id: session_id.to_string(),
                auto_approve,
            };
            let _ = sync_tx.send(event);
        }

        tracing::info!(
            "[PluginManager] Session mode set: session_id={}, auto_approve={}",
            session_id,
            auto_approve
        );
    }

    /// 查询会话自动授权模式
    ///
    /// 由 HTTP API `GET /api/plugin/session-mode` 调用。
    /// Python PreToolUse hook 通过此接口查询是否自动授权。
    pub async fn get_auto_mode(&self, session_id: &str) -> bool {
        let modes = self.auto_modes.read().await;
        modes.get(session_id).copied().unwrap_or(false)
    }

    /// 移除会话自动授权模式（会话结束时清理）
    pub async fn remove_auto_mode(&self, session_id: &str) {
        let mut modes = self.auto_modes.write().await;
        modes.remove(session_id);
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_states_store() {
        let manager = PluginManager::new();

        // 初始状态为空
        assert!(manager.get_task_status("session-1").await.is_none());

        // 直接写入内部状态（不触发广播，避免 AppContext 依赖）
        {
            let mut states = manager.task_states.write().await;
            states.insert(
                "session-1".to_string(),
                TaskStateEntry {
                    task_status: TaskStatus::InProgress,
                    task_reason: Some("Working".to_string()),
                    task_questions: None,
                    updated_at: Utc::now(),
                },
            );
        }

        assert_eq!(manager.get_task_status("session-1").await, Some(TaskStatus::InProgress));
        assert_eq!(
            manager.get_task_reason("session-1").await,
            Some("Working".to_string())
        );

        // 移除
        manager.remove_task_status("session-1").await;
        assert!(manager.get_task_status("session-1").await.is_none());
    }

    #[test]
    fn test_task_status_serde_format() {
        // 验证 serde 序列化输出 snake_case
        assert_eq!(
            serde_json::to_string(&TaskStatus::InProgress).unwrap().trim_matches('"'),
            "in_progress"
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Idle).unwrap().trim_matches('"'),
            "idle"
        );
    }

    #[tokio::test]
    async fn test_auto_modes() {
        let manager = PluginManager::new();

        // 默认为手动模式
        assert!(!manager.get_auto_mode("session-1").await);

        // 直接写入内部状态（不触发广播，避免 AppContext 依赖）
        {
            let mut modes = manager.auto_modes.write().await;
            modes.insert("session-1".to_string(), true);
        }

        assert!(manager.get_auto_mode("session-1").await);

        // 移除
        manager.remove_auto_mode("session-1").await;
        assert!(!manager.get_auto_mode("session-1").await);
    }
}
