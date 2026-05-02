//! Tauri Commands
//!
//! 暴露给前端的 Tauri 命令接口

use crate::auth::{PairingCode, PairingService};
use crate::db::{Database, QuickAction, SessionConfig};
use crate::discovery::{DiscoveredDevice, DiscoveryService};
use crate::Result;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

// ==================== WSL Commands (Desktop Only) ====================

/// 获取已安装的 WSL 发行版
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn list_wsl_distributions() -> Result<Vec<crate::pty::WslDistro>> {
    crate::pty::list_distributions()
}

/// 检查 WSL 是否可用
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn is_wsl_available() -> bool {
    crate::pty::is_wsl_available()
}

// ==================== Tmux Commands (Desktop Only) ====================

/// 获取 Tmux 会话列表
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn list_tmux_sessions() -> Result<Vec<crate::pty::TmuxSession>> {
    crate::pty::list_sessions()
}

/// 检查 Tmux 是否可用
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn is_tmux_available() -> bool {
    crate::pty::is_tmux_available()
}

/// 创建 Tmux 会话
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn create_tmux_session(name: String, command: Option<String>) -> Result<()> {
    crate::pty::create_session(&name, command.as_deref())
}

// ==================== Session Config Commands ====================

/// 创建会话配置
#[tauri::command]
pub async fn create_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    tmux_session: Option<String>,
) -> Result<SessionConfig> {
    let config = SessionConfig::new(name, environment, working_dir, command);
    let mut config = config;
    config.wsl_distro = wsl_distro;
    config.tmux_session = tmux_session;

    let db = db.lock().await;
    db.create_session_config(&config)?;
    Ok(config)
}

/// 获取所有会话配置
#[tauri::command]
pub async fn list_session_configs(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<SessionConfig>> {
    let db = db.lock().await;
    db.get_session_configs()
}

/// 获取单个会话配置
#[tauri::command]
pub async fn get_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<Option<SessionConfig>> {
    let db = db.lock().await;
    db.get_session_config(&id)
}

/// 删除会话配置
#[tauri::command]
pub async fn delete_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.delete_session_config(&id)
}

/// 更新会话配置
#[tauri::command]
pub async fn update_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    tmux_session: Option<String>,
    auto_start: Option<bool>,
) -> Result<SessionConfig> {
    let db = db.lock().await;
    let mut config = db
        .get_session_config(&id)?
        .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", id)))?;

    config.name = name;
    config.environment = environment;
    config.wsl_distro = wsl_distro;
    config.working_dir = working_dir;
    config.command = command;
    config.tmux_session = tmux_session;
    config.auto_start = auto_start.unwrap_or(false);

    db.update_session_config(&config)?;
    Ok(config)
}

// ==================== Session Commands (Desktop Only) ====================

/// 启动会话
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn start_session(
    session_manager: State<'_, Arc<crate::session::SessionManager>>,
    config_id: String,
) -> Result<String> {
    tracing::info!("start_session called with config_id: {}", config_id);
    let result = session_manager.create_session(&config_id).await;
    match result {
        Ok(id) => {
            tracing::info!("Session created successfully: {}", id);
            Ok(id)
        }
        Err(e) => {
            tracing::error!("Failed to create session: {}", e);
            Err(e)
        }
    }
}

/// 获取会话列表
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn list_sessions(
    session_manager: State<'_, Arc<crate::session::SessionManager>>,
) -> Result<Vec<crate::session::SessionInfo>> {
    Ok(session_manager.list_sessions().await)
}

/// 终止会话
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn kill_session(
    session_manager: State<'_, Arc<crate::session::SessionManager>>,
    session_id: String,
) -> Result<()> {
    session_manager.kill_session(&session_id).await
}

/// 调整会话终端大小
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn resize_session(
    session_manager: State<'_, Arc<crate::session::SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    session_manager.resize_session(&session_id, cols, rows).await
}

// ==================== PTY Input Commands (Desktop Only) ====================

/// 输入数据到会话
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn write_to_session(
    session_manager: State<'_, Arc<crate::session::SessionManager>>,
    session_id: String,
    data: String,
) -> Result<()> {
    session_manager.write_input(&session_id, &data).await
}

/// 发送特殊键
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn send_special_key(
    session_manager: State<'_, Arc<crate::session::SessionManager>>,
    session_id: String,
    key: String,
) -> Result<()> {
    session_manager.send_special_key(&session_id, &key).await
}

// ==================== Discovery Commands ====================

/// 开始设备发现
#[tauri::command]
pub async fn start_discovery(
    discovery_service: State<'_, Arc<DiscoveryService>>,
) -> Result<()> {
    discovery_service.start_discovery()
}

/// 获取已发现的设备
#[tauri::command]
pub async fn get_discovered_devices(
    discovery_service: State<'_, Arc<DiscoveryService>>,
) -> Result<Vec<DiscoveredDevice>> {
    Ok(discovery_service.get_discovered_devices().await)
}

/// 开始广播服务
#[tauri::command]
pub async fn start_broadcast(
    discovery_service: State<'_, Arc<DiscoveryService>>,
    service_name: String,
    port: u16,
) -> Result<()> {
    discovery_service.start_broadcast(&service_name, port)
}

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
) -> Result<Vec<crate::db::Pairing>> {
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

// ==================== Quick Actions Commands ====================

/// 获取快捷指令
#[tauri::command]
pub async fn list_quick_actions(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<QuickAction>> {
    let db = db.lock().await;
    db.get_quick_actions()
}

/// 创建快捷指令
#[tauri::command]
pub async fn create_quick_action(
    db: State<'_, Arc<Mutex<Database>>>,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<QuickAction> {
    let mut action = QuickAction::new(name, content);
    action.icon = icon;
    action.color = color;

    let db = db.lock().await;
    db.create_quick_action(&action)?;
    Ok(action)
}

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(
    app_handle: tauri::AppHandle,
) -> crate::Result<crate::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.json"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    crate::config::AppConfig::load(&config_path)
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

/// 保存应用设置
#[tauri::command]
pub async fn save_app_settings(
    app_handle: tauri::AppHandle,
    settings: crate::config::AppConfig,
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

/// 获取本地 IP 地址
#[tauri::command]
pub fn get_local_ip_addresses() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter(|(_, ip)| {
                    // 过滤掉回环地址和链路本地地址
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

// ==================== Android Specific Commands ====================

/// 获取 Android 状态栏高度（像素）
/// 通过 JNI 调用 Android API 获取系统状态栏高度
#[cfg(target_os = "android")]
#[tauri::command]
pub fn get_status_bar_height(app_handle: tauri::AppHandle) -> Result<u32> {
    // 通过 Android Activity 获取状态栏高度
    // 使用 jni 调用 Android API
    use tauri::Manager;

    let window = app_handle.webview_windows().get("main");
    if let Some(window) = window {
        // Android 上通过 WebView 的安全区域获取
        // 实际值会在前端通过 CSS env(safe-area-inset-top) 获取
        // 这里返回 0，前端会使用 CSS 变量
        return Ok(0);
    }

    Ok(0)
}

/// 非 Android 平台返回 0
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn get_status_bar_height() -> Result<u32> {
    Ok(0)
}

/// 设置 Android 屏幕方向
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn set_screen_orientation(
    app_handle: tauri::AppHandle,
    orientation: String,
) -> Result<()> {
    // 通过 JNI 设置 Activity 的屏幕方向
    // orientation: "portrait", "landscape", "unspecified"
    tracing::info!("Setting screen orientation to: {}", orientation);

    // TODO: 实现 JNI 调用 Android Activity.setRequestedOrientation
    // 需要在 lib.rs 中注册 Android 专用模块

    Ok(())
}

/// 非 Android 平台忽略
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn set_screen_orientation(_orientation: String) -> Result<()> {
    Ok(())
}

/// 保持屏幕唤醒（防止锁屏）
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn keep_screen_awake(
    app_handle: tauri::AppHandle,
    enabled: bool,
) -> Result<()> {
    tracing::info!("Setting screen awake: {}", enabled);

    // TODO: 通过 JNI 调用 Activity.getWindow().setKeepScreenOn()

    Ok(())
}

/// 非 Android 平台忽略
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn keep_screen_awake(_enabled: bool) -> Result<()> {
    Ok(())
}
