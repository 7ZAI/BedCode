//! BedCode Mobile - Library Entry Point

pub mod auth;
pub mod commands;
pub mod connection;
pub mod enums;
pub mod handler;
pub mod model;
pub mod plugin;
pub mod router;
pub mod session;
pub mod state;
pub mod system;
pub mod mdns;

// Re-export core types
pub use system::error::{AppError, Result};
pub use system::config;

use connection::PairingService;
use std::sync::Arc;
use tauri::Manager;
use android_logger::Config;
use log::LevelFilter;

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use crate::system::settings::SettingsManager;

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
        .plugin(crate::plugin::android_plugins::init())
        .setup(|app| {
            tracing::info!("BedCode setup starting...");
            tracing::info!("Plugins initialized");

            let app_handle = app.handle();

            // 初始化移动端设置管理器 (JSON 文件存储)
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            let settings_manager = Arc::new(SettingsManager::new(&app_data_dir)?);
            app.manage(settings_manager.clone());

            // 初始化插件 KV 存储
            let plugin_storage = Arc::new(
                crate::plugin::storage::PluginStorage::new(&app_data_dir)
            );

            // 创建插件数据库连接（WASM Host Function 使用）
            let db_path = app_data_dir.join("bedcode_plugins.db");
            let plugin_db = Arc::new(tokio::sync::Mutex::new(
                rusqlite::Connection::open(&db_path)
                    .map_err(|e| anyhow::anyhow!("Failed to open plugin DB: {}", e))?
            ));

            // 创建 WASM 运行时
            let wasm_runtime = Arc::new(
                crate::plugin::wasm_runtime::WasmRuntime::new(
                    plugin_db.clone(),
                    plugin_storage.clone(),
                    Arc::new(app_handle.clone()),
                )
                .map_err(|e| anyhow::anyhow!("Failed to create WasmRuntime: {}", e))?
            );

            // 创建 WASM 宿主上下文
            let wasm_host_ctx = Arc::new(
                crate::plugin::wasm_runtime::WasmHostContext::new(
                    plugin_db,
                    plugin_storage,
                    Arc::new(app_handle.clone()),
                )
            );

            // 初始化插件管理器
            let plugin_manager = Arc::new(
                crate::plugin::manager::PluginManager::new(
                    &app_data_dir,
                    settings_manager.clone(),
                    wasm_runtime,
                    wasm_host_ctx,
                )
            );
            app.manage(plugin_manager.clone());

            // 解压 APK assets 中的内置插件
            if let Err(e) = crate::plugin::loader::PluginLoader::extract_apk_plugins(
                &app_data_dir, &app_handle
            ) {
                tracing::warn!("Failed to extract APK plugins: {}", e);
            }

            // 异步：扫描加载 + 自动激活
            {
                let pm = plugin_manager;
                let ah = app_handle.clone();
                tokio::spawn(async move {
                    pm.scan_and_load().await;
                    pm.load_all(&ah).await;
                });
            }

            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service);

            // 初始化 mDNS 管理器
            let mdns_discovery = Arc::new(tokio::sync::RwLock::new(crate::mdns::discovery::MdnsDiscovery::new()));
            app.manage(mdns_discovery);
            let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(crate::mdns::advertiser::MdnsAdvertiser::new()));
            app.manage(mdns_advertiser);

            tracing::info!("BedCode Mobile started successfully!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Token Commands (merged into connection)
            commands::connection::ws_set_token,
            commands::connection::ws_get_token,
            commands::connection::ws_clear_token,
            // Connection Commands
            commands::connection::ws_connect,
            commands::connection::ws_disconnect,
            commands::connection::ws_get_status,
            commands::connection::ws_is_connected,
            commands::connection::ws_reconnect,
            // Auth Commands
            commands::auth::ws_get_auth_status,
            commands::auth::ws_authenticate,
            commands::auth::ws_request_pairing,
            commands::auth::ws_verify_pairing_code,
            commands::auth::ws_authenticate_with_qr,
            // Session Commands
            commands::session::ws_load_sessions,
            commands::session::ws_join_session,
            commands::session::ws_leave_session,
            commands::session::ws_subscribe_session,
            commands::session::ws_start_session,
            commands::session::ws_stop_session,
            commands::session::ws_remove_session,
            commands::session::ws_load_session_configs,
            // Terminal Commands
            commands::terminal::ws_send_input_async,
            commands::terminal::ws_send_message,
            commands::terminal::ws_send_and_wait,
            commands::terminal::ws_resize_terminal,
            // Pairing
            system::commands::generate_pairing_code,
            system::commands::get_current_pairing_code,
            system::commands::verify_pairing_code,
            system::commands::clear_pairing_code,
            // Settings (移动端使用 JSON 文件)
            commands::mobile_commands::get_all_db_settings_mobile,
            commands::mobile_commands::set_db_setting_mobile,
            // App Settings
            system::commands::get_app_settings,
            system::commands::save_app_settings,
            // Utility
            system::commands::ping,
            system::commands::get_app_version,
            system::commands::get_local_ip_addresses,
            // Android Specific
            commands::android::set_screen_orientation,
            commands::android::keep_screen_awake,
            commands::android::open_url_in_browser,
            // Session Config (移动端使用内存存储)
            commands::mobile_commands::list_session_configs_mobile,
            commands::mobile_commands::get_session_config_mobile,
            // mDNS
            commands::mdns::mdns_start_discovery,
            commands::mdns::mdns_stop_discovery,
            commands::mdns::mdns_get_discovered_services,
            commands::mdns::mdns_start_advertise,
            commands::mdns::mdns_stop_advertise,
            // Plugin Commands
            crate::plugin::commands::plugin_list_loaded,
            crate::plugin::commands::plugin_get_info,
            crate::plugin::commands::plugin_activate,
            crate::plugin::commands::plugin_deactivate,
            crate::plugin::commands::plugin_is_enabled,
            crate::plugin::commands::plugin_set_enabled,
            crate::plugin::commands::plugin_mark_error,
            crate::plugin::commands::plugin_storage_get,
            crate::plugin::commands::plugin_storage_set,
            crate::plugin::commands::plugin_storage_delete,
            crate::plugin::commands::plugin_download,
            crate::plugin::commands::reload_wasm_plugin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    tracing::info!("BedCode application closed");
}
