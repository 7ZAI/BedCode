//! Mobile-Only Commands
//!
//! 移动端专用命令 - 使用内存存储和 JSON 文件存储
//! 从 shared/system/commands.rs 迁移而来

use crate::shared::auth::PairingCode;
use crate::shared::db::{QuickAction, SessionConfig};
use crate::Result;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::mobile::system::settings::SettingsManager;
use chrono::Utc;
use uuid::Uuid;

/// 移动端内存存储的快捷指令
static QUICK_ACTIONS: std::sync::LazyLock<Arc<RwLock<Vec<QuickAction>>>> =
    std::sync::LazyLock::new(|| {
        Arc::new(RwLock::new(vec![
            QuickAction {
                id: Uuid::new_v4().to_string(),
                name: "继续".to_string(),
                content: "请继续".to_string(),
                icon: Some("▶️".to_string()),
                color: Some("#22c55e".to_string()),
                category: None,
                sort_order: 0,
                created_at: Utc::now(),
            },
            QuickAction {
                id: Uuid::new_v4().to_string(),
                name: "解释代码".to_string(),
                content: "请解释这段代码的作用".to_string(),
                icon: Some("📝".to_string()),
                color: Some("#3b82f6".to_string()),
                category: None,
                sort_order: 1,
                created_at: Utc::now(),
            },
        ]))
    });

/// 移动端内存存储的会话配置
static SESSION_CONFIGS: std::sync::LazyLock<Arc<RwLock<Vec<SessionConfig>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

// ==================== Quick Actions (内存存储) ====================

/// 获取快捷指令 (移动端内存存储)
#[tauri::command]
pub async fn list_quick_actions_mobile() -> Result<Vec<QuickAction>> {
    let actions = QUICK_ACTIONS.read().await;
    Ok(actions.clone())
}

/// 创建快捷指令 (移动端内存存储)
#[tauri::command]
pub async fn create_quick_action_mobile(
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<QuickAction> {
    let mut action = QuickAction::new(name, content);
    action.icon = icon;
    action.color = color;

    let mut actions = QUICK_ACTIONS.write().await;
    actions.push(action.clone());
    Ok(action)
}

/// 更新快捷指令 (移动端内存存储)
#[tauri::command]
pub async fn update_quick_action_mobile(
    id: String,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<QuickAction> {
    let mut actions = QUICK_ACTIONS.write().await;
    let action = actions
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| crate::AppError::NotFound(format!("Quick action not found: {}", id)))?;

    action.name = name;
    action.content = content;
    action.icon = icon;
    action.color = color;

    Ok(action.clone())
}

/// 删除快捷指令 (移动端内存存储)
#[tauri::command]
pub async fn delete_quick_action_mobile(id: String) -> Result<()> {
    let mut actions = QUICK_ACTIONS.write().await;
    actions.retain(|a| a.id != id);
    Ok(())
}

// ==================== Settings (JSON 文件存储) ====================

/// 获取所有设置 (移动端 JSON 文件存储)
#[tauri::command]
pub async fn get_all_db_settings_mobile(
    settings_manager: State<'_, SettingsManager>,
) -> Result<Vec<crate::shared::db::Setting>> {
    let settings = settings_manager.get_all().await?;
    let now = Utc::now();
    Ok(settings
        .into_iter()
        .map(|(key, value)| crate::shared::db::Setting {
            key,
            value,
            updated_at: now,
        })
        .collect())
}

/// 设置配置项 (移动端 JSON 文件存储)
#[tauri::command]
pub async fn set_db_setting_mobile(
    settings_manager: State<'_, SettingsManager>,
    key: String,
    value: String,
) -> Result<()> {
    settings_manager.set(key, value).await
}

// ==================== Session Configs (内存存储) ====================

/// 获取所有会话配置 (移动端内存存储)
#[tauri::command]
pub async fn list_session_configs_mobile() -> Result<Vec<SessionConfig>> {
    let configs = SESSION_CONFIGS.read().await;
    Ok(configs.clone())
}

/// 获取单个会话配置 (移动端内存存储)
#[tauri::command]
pub async fn get_session_config_mobile(id: String) -> Result<Option<SessionConfig>> {
    let configs = SESSION_CONFIGS.read().await;
    Ok(configs.iter().find(|c| c.id == id).cloned())
}
