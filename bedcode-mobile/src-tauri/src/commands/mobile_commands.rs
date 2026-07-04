//! Mobile-Only Commands
//!
//! 移动端专用命令 - 使用内存存储和 JSON 文件存储

use crate::model::data::{SessionConfig, Setting};
use crate::Result;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::system::settings::SettingsManager;
use chrono::Utc;

/// 移动端内存存储的会话配置
static SESSION_CONFIGS: std::sync::LazyLock<Arc<RwLock<Vec<SessionConfig>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

// ==================== Settings (JSON 文件存储) ====================

/// 获取所有设置 (移动端 JSON 文件存储)
#[tauri::command]
pub async fn get_all_db_settings_mobile(
    settings_manager: State<'_, SettingsManager>,
) -> Result<Vec<Setting>> {
    let settings = settings_manager.get_all().await?;
    let now = Utc::now();
    Ok(settings
        .into_iter()
        .map(|(key, value)| Setting {
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
