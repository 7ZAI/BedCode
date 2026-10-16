//! Mobile-Only Commands
//!
//! 移动端专用命令 - 使用 JSON 文件存储

use crate::model::data::Setting;
use crate::Result;
use std::sync::Arc;
use tauri::State;

use crate::system::settings::SettingsManager;
use chrono::Utc;

// ==================== Settings (JSON 文件存储) ====================

/// 获取所有设置 (移动端 JSON 文件存储)
#[tauri::command]
pub async fn get_all_db_settings_mobile(settings_manager: State<'_, Arc<SettingsManager>>) -> Result<Vec<Setting>> {
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
    settings_manager: State<'_, Arc<SettingsManager>>,
    key: String,
    value: String,
) -> Result<()> {
    settings_manager.set(key, value).await
}
