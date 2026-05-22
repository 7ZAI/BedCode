//! Shared Tauri Commands
//!
//! 跨平台命令 - 桌面端和移动端都可用

use crate::shared::auth::PairingCode;
use crate::shared::db::{QuickAction, SessionConfig};
use crate::shared::db::Database;
use crate::Result;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::desktop::server::services::PairingService;
#[cfg(any(target_os = "android", target_os = "ios"))]
use crate::mobile::pairing_service::PairingService;

// ==================== Pairing Commands ====================

/// 生成配对码
#[tauri::command]
pub async fn generate_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
) -> Result<PairingCode> {
    Ok(pairing_service.generate_code().await)
}

/// 获取当前配对码
#[tauri::command]
pub async fn get_current_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
) -> Result<Option<PairingCode>> {
    Ok(pairing_service.get_current_code().await)
}

/// 验证配对码
#[tauri::command]
pub async fn verify_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
    code: String,
) -> Result<bool> {
    Ok(pairing_service.verify_code(&code).await)
}

/// 清除当前配对码（用于取消配对或配对码过期）
#[tauri::command]
pub async fn clear_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
) -> Result<()> {
    pairing_service.clear_code().await;
    Ok(())
}

/// 获取已配对设备
#[tauri::command]
pub async fn list_paired_devices(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<crate::shared::db::Pairing>> {
    let db = db.lock().await;
    db.get_pairings()
}

/// 移除配对设备
#[tauri::command]
pub async fn remove_paired_device(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.remove_pairing(&id)
}

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(
    app_handle: tauri::AppHandle,
) -> crate::Result<crate::shared::system::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.json"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    crate::shared::system::config::AppConfig::load(&config_path)
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

/// 保存应用设置
#[tauri::command]
pub async fn save_app_settings(
    app_handle: tauri::AppHandle,
    settings: crate::shared::system::config::AppConfig,
) -> crate::Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.json"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    settings.save(&config_path)?;

    tracing::info!("App settings saved to {:?}", config_path);
    Ok(())
}

// ==================== Utility Commands ====================

/// 测试命令
#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

/// 获取应用版本
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 获取自应用启动以来的耗时（毫秒）
/// 用于前端计算从进程启动到页面渲染完成的总耗时
#[tauri::command]
pub fn get_startup_time(start_time: State<'_, crate::AppStartTime>) -> u64 {
    start_time.0.elapsed().as_millis() as u64
}

/// 获取本地 IP 地址
#[tauri::command]
pub fn get_local_ip_addresses() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter(|(_, ip)| {
                    match ip {
                        std::net::IpAddr::V4(ipv4) => {
                            !ipv4.is_loopback() && !ipv4.is_link_local()
                        }
                        std::net::IpAddr::V6(ipv6) => {
                            !ipv6.is_loopback()
                        }
                    }
                })
                .map(|(_, ip)| ip.to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ==================== Mobile-Only Commands (In-Memory Storage) ====================

#[cfg(any(target_os = "android", target_os = "ios"))]
use crate::shared::system::settings::SettingsManager;
#[cfg(any(target_os = "android", target_os = "ios"))]
use chrono::Utc;
#[cfg(any(target_os = "android", target_os = "ios"))]
use tokio::sync::RwLock;
#[cfg(any(target_os = "android", target_os = "ios"))]
use uuid::Uuid;

/// 移动端内存存储的快捷指令
#[cfg(any(target_os = "android", target_os = "ios"))]
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
#[cfg(any(target_os = "android", target_os = "ios"))]
static SESSION_CONFIGS: std::sync::LazyLock<Arc<RwLock<Vec<SessionConfig>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

/// 获取快捷指令 (移动端内存存储)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn list_quick_actions_mobile() -> Result<Vec<QuickAction>> {
    let actions = QUICK_ACTIONS.read().await;
    Ok(actions.clone())
}

/// 创建快捷指令 (移动端内存存储)
#[cfg(any(target_os = "android", target_os = "ios"))]
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
#[cfg(any(target_os = "android", target_os = "ios"))]
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
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn delete_quick_action_mobile(id: String) -> Result<()> {
    let mut actions = QUICK_ACTIONS.write().await;
    actions.retain(|a| a.id != id);
    Ok(())
}

/// 获取所有设置 (移动端 JSON 文件存储)
#[cfg(any(target_os = "android", target_os = "ios"))]
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
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn set_db_setting_mobile(
    settings_manager: State<'_, SettingsManager>,
    key: String,
    value: String,
) -> Result<()> {
    settings_manager.set(key, value).await
}

/// 获取所有会话配置 (移动端内存存储)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn list_session_configs_mobile() -> Result<Vec<SessionConfig>> {
    let configs = SESSION_CONFIGS.read().await;
    Ok(configs.clone())
}

/// 获取单个会话配置 (移动端内存存储)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn get_session_config_mobile(id: String) -> Result<Option<SessionConfig>> {
    let configs = SESSION_CONFIGS.read().await;
    Ok(configs.iter().find(|c| c.id == id).cloned())
}

