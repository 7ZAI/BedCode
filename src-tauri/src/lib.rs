//! BedCode - Library Entry Point

// Shared modules - available on both desktop and mobile
pub mod shared;

// Desktop-only modules
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod desktop;

// Mobile-only modules
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod mobile;

// Re-export shared types
pub use shared::{AppError, Result};

// Re-export shared modules for testing
pub use shared::auth;
pub use shared::config;
pub use shared::db;
pub use shared::parser;
pub use shared::notify;
pub use shared::system;
pub use shared::websocket;

// Re-export desktop modules for testing
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::session;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::pty;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::server;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::plugin;

use shared::auth::qr_token::QrTokenManager;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use desktop::server::services::PairingService;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use desktop::server::services::QrTokenService;
#[cfg(any(target_os = "android", target_os = "ios"))]
use shared::auth::PairingService;
use shared::system::config::AppConfig;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use shared::db::Database;
use std::sync::Arc;
use tauri::Manager;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tokio::sync::Mutex;
#[cfg(target_os = "android")]
use android_logger::Config;
#[cfg(target_os = "android")]
use log::LevelFilter;
#[cfg(target_os = "android")]
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 初始化日志系统
#[allow(unused_variables)]
fn init_logging(_app_handle: &tauri::AppHandle) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        // Android: 使用 tracing-subscriber 输出到 stderr（logcat）
        // 同时初始化 android_logger 捕获 log crate 的输出
        android_logger::init_once(
            Config::default()
                .with_max_level(LevelFilter::Debug)
                .with_tag("BedCode")
        );

        // 使用 EnvFilter 过滤日志级别
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug"));

        // 初始化 tracing-subscriber，输出到 stderr（Android logcat）
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(std::io::stderr))
            .init();

        tracing::info!("BedCode Android logging initialized (tracing + log)");
    }

    #[cfg(not(target_os = "android"))]
    {
        init_logging_desktop(_app_handle)?;
    }

    Ok(())
}

#[cfg(not(target_os = "android"))]
fn init_logging_desktop(app_handle: &tauri::AppHandle) -> Result<()> {
    let log_dir = app_handle
        .path()
        .app_log_dir()
        .expect("Failed to get log directory");

    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("bedcode")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)
        .expect("Failed to create log file appender");

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true);

    #[cfg(debug_assertions)]
    {
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true)
            .with_target(true)
            .pretty();

        tracing_subscriber::registry()
            .with(file_layer)
            .with(console_layer)
            .with(tracing_subscriber::EnvFilter::new("debug"))
            .init();
    }

    #[cfg(not(debug_assertions))]
    {
        tracing_subscriber::registry()
            .with(file_layer)
            .with(tracing_subscriber::EnvFilter::new("info"))
            .init();
    }

    tracing::info!("Logging initialized. Log directory: {:?}", log_dir);
    tracing::info!("BedCode v{} starting...", env!("CARGO_PKG_VERSION"));

    Ok(())
}

/// Insert default quick actions (Desktop only)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn insert_default_quick_actions(db: &Database) -> Result<()> {
    let actions = db.get_quick_actions()?;
    if !actions.is_empty() {
        return Ok(());
    }

    let default_actions = [
        ("继续", "请继续", "▶️", "#22c55e"),
        ("解释代码", "请解释这段代码的作用", "📝", "#3b82f6"),
        ("修复 Bug", "请帮我修复这个 Bug", "🔧", "#a855f7"),
        ("提交代码", "请帮我提交代码", "📤", "#f97316"),
    ];

    for (name, content, icon, color) in default_actions {
        let mut action = shared::db::QuickAction::new(name.to_string(), content.to_string());
        action.icon = Some(icon.to_string());
        action.color = Some(color.to_string());
        db.create_quick_action(&action)?;
    }

    tracing::info!("Inserted default quick actions");
    Ok(())
}

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

// ==================== Desktop Entry Point ====================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn run() {
    use tauri::Emitter;

    let app_start = AppStartTime(std::time::Instant::now());
    let start = app_start.0;

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(move |app| {
            init_logging(app.handle())?;
            app.manage(app_start);

            let app_handle = app.handle();
            let config_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("config.json");

            let app_config = AppConfig::load(&config_path).unwrap_or_else(|e| {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                AppConfig::default()
            });
            let ws_port = app_config.network.port;

            let db_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("bedcode.db");

            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let db = Database::new(&db_path)?;
            db.init_schema()?;
            insert_default_quick_actions(&db)?;

            let db = Arc::new(Mutex::new(db));
            app.manage(db.clone());

            // 创建会话存储（通过 trait）
            let storage = Arc::new(desktop::session::SessionStorage::new(db.clone()));
            let session_manager = Arc::new(desktop::session::SessionManager::new(storage));
            app.manage(session_manager.clone());

            let plugin_manager = Arc::new(desktop::plugin::PluginManager::new(
                session_manager.output_tx(),
                db.clone(),
            ));

            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service.clone());

            let qr_manager = Arc::new(QrTokenService::new());
            app.manage(qr_manager.clone());

            use std::sync::Arc as StdArc;

            // 创建 BusinessMessageHandler 并注入依赖
            let business_handler = Arc::new(
                desktop::server::handlers::BusinessMessageHandler::new(
                    db.clone(),
                    pairing_service.clone(),
                    qr_manager.clone(),
                    Some(StdArc::new(app_handle.clone())),
                )
            );

            // 初始化并启动 WebSocketManager（需要在 async runtime 中）
            let ws_manager = desktop::websocket_manager::WebSocketManager::global();
            tauri::async_runtime::spawn(async move {
                ws_manager.init(Some(business_handler as Arc<dyn desktop::websocket_manager::BusinessHandler>)).await
                    .expect("Failed to initialize WebSocketManager");
                tracing::info!("[BedCode] Starting WebSocket server on port {}", ws_port);
                match ws_manager.start(ws_port).await {
                    Ok(_) => tracing::info!("[BedCode] WebSocket server started successfully"),
                    Err(e) => tracing::error!("[BedCode] WebSocket server failed to start: {}", e),
                }
            });

            let app_handle_clone = app_handle.clone();
            let ws_port_copy = ws_port;
            tauri::async_runtime::spawn(async move {
                let port_file = app_handle_clone
                    .path()
                    .app_data_dir()
                    .ok()
                    .map(|p| p.join("bedcode-port.txt"));

                if let Some(port_file) = port_file {
                    if let Some(parent) = port_file.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if let Err(e) = tokio::fs::write(&port_file, ws_port_copy.to_string()).await {
                        tracing::warn!("Failed to write port file: {}", e);
                    } else {
                        tracing::info!("Wrote port file: {}", port_file.display());
                    }
                }
            });

            let app_handle_clone = app_handle.clone();
            let session_manager_clone = session_manager.clone();
            tauri::async_runtime::spawn(async move {
                let mut rx = session_manager_clone.subscribe_output();
                while let Ok(event) = rx.recv().await {
                    if let Err(e) = app_handle_clone.emit("pty-output", &event) {
                        tracing::error!("Failed to emit output event: {}", e);
                    }
                }
            });

            let app_handle_clone2 = app_handle.clone();
            let session_manager_clone2 = session_manager.clone();
            tauri::async_runtime::spawn(async move {
                let mut rx = session_manager_clone2.subscribe_status();
                while let Ok(event) = rx.recv().await {
                    if let Err(e) = app_handle_clone2.emit("session-status-changed", &event) {
                        tracing::error!("Failed to emit status event: {}", e);
                    }
                }
            });

            let app_handle_clone3 = app_handle.clone();
            let session_manager_clone3 = session_manager.clone();
            tauri::async_runtime::spawn(async move {
                let mut rx = session_manager_clone3.subscribe_restart();
                while let Ok(event) = rx.recv().await {
                    if let Err(e) = app_handle_clone3.emit("session-restarted", &event) {
                        tracing::error!("Failed to emit restart event: {}", e);
                    }
                }
            });

            setup_tray(app_handle)?;

            let window = app_handle.get_webview_window("main").expect("Failed to get main window");
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    tracing::info!("Window close requested, shutting down...");
                }
            });

            let init_elapsed = start.elapsed();
            tracing::info!("BedCode (Desktop) initialized - WebSocket server on port {} (后端初始化耗时: {}ms)", ws_port, init_elapsed.as_millis());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // WSL
            desktop::commands::list_wsl_distributions,
            desktop::commands::is_wsl_available,
            // Tmux
            desktop::commands::list_tmux_sessions,
            desktop::commands::is_tmux_available,
            desktop::commands::create_tmux_session,
            // Session Config
            shared::system::commands::create_session_config,
            shared::system::commands::list_session_configs,
            shared::system::commands::get_session_config,
            shared::system::commands::delete_session_config,
            shared::system::commands::update_session_config,
            // Session
            desktop::commands::start_session,
            desktop::commands::list_sessions,
            desktop::commands::get_session,
            desktop::commands::kill_session,
            desktop::commands::delete_session,
            desktop::commands::restart_session,
            desktop::commands::resize_session,
            // PTY Input
            desktop::commands::write_to_session,
            desktop::commands::send_special_key,
            // Pairing
            shared::system::commands::generate_pairing_code,
            shared::system::commands::get_current_pairing_code,
            shared::system::commands::verify_pairing_code,
            shared::system::commands::clear_pairing_code,
            shared::system::commands::list_paired_devices,
            shared::system::commands::remove_paired_device,
            // QR Code
            shared::system::commands::generate_qr_code,
            shared::system::commands::clear_qr_code,
            shared::system::commands::get_qr_connection_info,
            shared::system::commands::get_qr_token_ttl,
            shared::system::commands::set_qr_token_ttl,
            // Quick Actions
            shared::system::commands::list_quick_actions,
            shared::system::commands::create_quick_action,
            shared::system::commands::update_quick_action,
            shared::system::commands::delete_quick_action,
            shared::system::commands::get_all_db_settings,
            shared::system::commands::set_db_setting,
            // Settings
            shared::system::commands::get_app_settings,
            shared::system::commands::save_app_settings,
            // Utility
            shared::system::commands::ping,
            shared::system::commands::get_app_version,
            shared::system::commands::get_startup_time,
            shared::system::commands::get_local_ip_addresses,
            desktop::commands::get_connected_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Setup system tray (desktop only)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn setup_tray(app: &tauri::AppHandle) -> Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide", "隐藏窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    tracing::info!("System tray initialized");
    Ok(())
}

// ==================== Mobile Entry Point ====================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn run() {
    use crate::shared::system::settings::SettingsManager;
    use android_logger::Config;
    use log::LevelFilter;

    // 尽可能早地初始化日志
    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Debug)
            .with_tag("BedCode")
    );
    log::info!("BedCode Mobile early logging init");

    log::info!("Building Tauri application...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            // 日志已在 run() 中早期初始化，这里不再重复初始化
            log::info!("BedCode setup starting...");

            // 插件初始化日志
            log::info!("Plugins initialized");

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

            log::info!("BedCode Mobile started successfully!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Mobile WebSocket Commands
            mobile::commands::ws_connect,
            mobile::commands::ws_disconnect,
            mobile::commands::ws_get_status,
            mobile::commands::ws_is_connected,
            mobile::commands::ws_get_auth_status,
            mobile::commands::ws_authenticate,
            mobile::commands::ws_request_pairing,
            mobile::commands::ws_verify_pairing_code,
            mobile::commands::ws_authenticate_with_qr,
            mobile::commands::ws_load_sessions,
            mobile::commands::ws_start_session,
            mobile::commands::ws_stop_session,
            mobile::commands::ws_send_input,
            mobile::commands::ws_send_message,
            mobile::commands::ws_send_and_wait,
            mobile::commands::ws_resize_terminal,
            mobile::commands::ws_load_session_configs,
            // Pairing
            shared::system::commands::generate_pairing_code,
            shared::system::commands::get_current_pairing_code,
            shared::system::commands::verify_pairing_code,
            shared::system::commands::clear_pairing_code,
            shared::system::commands::list_paired_devices,
            shared::system::commands::remove_paired_device,
            // Quick Actions (移动端使用内存存储)
            shared::system::commands::list_quick_actions_mobile,
            shared::system::commands::create_quick_action_mobile,
            shared::system::commands::update_quick_action_mobile,
            shared::system::commands::delete_quick_action_mobile,
            // Settings (移动端使用 JSON 文件)
            shared::system::commands::get_all_db_settings_mobile,
            shared::system::commands::set_db_setting_mobile,
            // App Settings
            shared::system::commands::get_app_settings,
            shared::system::commands::save_app_settings,
            // Utility
            shared::system::commands::ping,
            shared::system::commands::get_app_version,
            shared::system::commands::get_local_ip_addresses,
            // Android Specific
            mobile::commands::get_status_bar_height,
            mobile::commands::set_screen_orientation,
            mobile::commands::keep_screen_awake,
            // Session Config (移动端使用内存存储)
            shared::system::commands::list_session_configs_mobile,
            shared::system::commands::get_session_config_mobile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // 这行永远不会执行，因为 run() 会阻塞
    log::info!("BedCode application closed");
}