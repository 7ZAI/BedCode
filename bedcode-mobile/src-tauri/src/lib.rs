//! BedCode Mobile - Library Entry Point

pub mod shared;
pub mod mobile;

// Re-export shared types
pub use shared::{AppError, Result};

// Re-export shared modules for testing
pub use shared::auth;
pub use shared::config;
pub use shared::db;
pub use shared::system;

use mobile::remote::PairingService;
use std::sync::Arc;
use tauri::Manager;
use android_logger::Config;
use log::LevelFilter;

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use crate::mobile::system::settings::SettingsManager;

    // 尽可能早地初始化日志
    // tracing 的 "log" feature 将 tracing:: 宏自动转发到 log crate
    // android_logger 将 log:: 输出发送到 adb logcat
    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Debug)
            .with_tag("BedCode")
    );
    tracing::info!("BedCode Mobile early logging init (tracing → log → logcat)");

    tracing::info!("Building Tauri application...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_edge_to_edge::init())
        .plugin(tauri_plugin_http::init())
        .setup(|app| {
            tracing::info!("BedCode setup starting...");
            tracing::info!("Plugins initialized");

            let app_handle = app.handle();

            // 初始化移动端设置管理器 (JSON 文件存储)
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            let settings_manager = SettingsManager::new(&app_data_dir)?;
            app.manage(settings_manager);

            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service);

            tracing::info!("BedCode Mobile started successfully!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Mobile Token Commands
            mobile::commands::token::ws_set_token,
            mobile::commands::token::ws_get_token,
            mobile::commands::token::ws_clear_token,
            // Mobile WebSocket Commands
            mobile::commands::connection::ws_connect,
            mobile::commands::connection::ws_disconnect,
            mobile::commands::connection::ws_get_status,
            mobile::commands::connection::ws_is_connected,
            mobile::commands::connection::ws_reconnect,
            // Mobile Auth Commands
            mobile::commands::auth::ws_get_auth_status,
            mobile::commands::auth::ws_authenticate,
            mobile::commands::auth::ws_request_pairing,
            mobile::commands::auth::ws_verify_pairing_code,
            mobile::commands::auth::ws_authenticate_with_qr,
            // Mobile Session Commands
            mobile::commands::session::ws_load_sessions,
            mobile::commands::session::ws_join_session,
            mobile::commands::session::ws_leave_session,
            mobile::commands::session::ws_subscribe_session,
            mobile::commands::session::ws_start_session,
            mobile::commands::session::ws_stop_session,
            mobile::commands::session::ws_remove_session,
            mobile::commands::session::ws_load_session_configs,
            // Mobile Terminal Commands
            mobile::commands::terminal::ws_send_input_async,
            mobile::commands::terminal::ws_send_message,
            mobile::commands::terminal::ws_send_and_wait,
            mobile::commands::terminal::ws_resize_terminal,
            // Pairing
            shared::system::commands::generate_pairing_code,
            shared::system::commands::get_current_pairing_code,
            shared::system::commands::verify_pairing_code,
            shared::system::commands::clear_pairing_code,
            shared::system::commands::list_paired_devices,
            shared::system::commands::remove_paired_device,
            // Quick Actions (移动端使用内存存储)
            mobile::commands::mobile_commands::list_quick_actions_mobile,
            mobile::commands::mobile_commands::create_quick_action_mobile,
            mobile::commands::mobile_commands::update_quick_action_mobile,
            mobile::commands::mobile_commands::delete_quick_action_mobile,
            // Settings (移动端使用 JSON 文件)
            mobile::commands::mobile_commands::get_all_db_settings_mobile,
            mobile::commands::mobile_commands::set_db_setting_mobile,
            // App Settings
            shared::system::commands::get_app_settings,
            shared::system::commands::save_app_settings,
            // Utility
            shared::system::commands::ping,
            shared::system::commands::get_app_version,
            shared::system::commands::get_local_ip_addresses,
            // Android Specific
            mobile::commands::android::set_screen_orientation,
            mobile::commands::android::keep_screen_awake,
            mobile::commands::android::open_url_in_browser,
            // Session Config (移动端使用内存存储)
            mobile::commands::mobile_commands::list_session_configs_mobile,
            mobile::commands::mobile_commands::get_session_config_mobile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    tracing::info!("BedCode application closed");
}
