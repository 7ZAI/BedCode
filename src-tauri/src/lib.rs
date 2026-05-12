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

use shared::auth::{PairingService, QrTokenManager};
use shared::config::AppConfig;
use shared::db::Database;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 初始化日志系统
fn init_logging(app_handle: &tauri::AppHandle) -> Result<()> {
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

/// Insert default quick actions
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
    use desktop::session::SessionManager;
    use desktop::websocket::WebSocketServer;
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

            let session_manager = Arc::new(SessionManager::new(db.clone()));
            app.manage(session_manager.clone());

            let plugin_manager = Arc::new(desktop::plugin::PluginManager::new(
                session_manager.output_tx(),
                db.clone(),
            ));

            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service.clone());

            let qr_manager = Arc::new(QrTokenManager::new());
            app.manage(qr_manager.clone());

            let mut ws_server = WebSocketServer::new(
                ws_port,
                session_manager.clone(),
                plugin_manager.clone(),
                db.clone(),
                pairing_service.clone(),
                qr_manager.clone(),
            );

            use std::sync::Arc as StdArc;
            ws_server.set_app_handle(StdArc::new(app_handle.clone()));

            let ws_server = Arc::new(ws_server);
            app.manage(ws_server.clone());

            let ws_server_clone = ws_server.clone();
            tauri::async_runtime::spawn(async move {
                tracing::info!("Starting WebSocket server on port {}", ws_port);
                if let Err(e) = ws_server_clone.start().await {
                    tracing::error!("WebSocket server error: {}", e);
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
            shared::commands::create_session_config,
            shared::commands::list_session_configs,
            shared::commands::get_session_config,
            shared::commands::delete_session_config,
            shared::commands::update_session_config,
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
            shared::commands::generate_pairing_code,
            shared::commands::get_current_pairing_code,
            shared::commands::verify_pairing_code,
            shared::commands::clear_pairing_code,
            shared::commands::list_paired_devices,
            shared::commands::remove_paired_device,
            // QR Code
            shared::commands::generate_qr_code,
            shared::commands::clear_qr_code,
            shared::commands::get_qr_connection_info,
            shared::commands::get_qr_token_ttl,
            shared::commands::set_qr_token_ttl,
            // Quick Actions
            shared::commands::list_quick_actions,
            shared::commands::create_quick_action,
            shared::commands::update_quick_action,
            shared::commands::delete_quick_action,
            shared::commands::get_all_db_settings,
            shared::commands::set_db_setting,
            // Settings
            shared::commands::get_app_settings,
            shared::commands::save_app_settings,
            // Utility
            shared::commands::ping,
            shared::commands::get_app_version,
            shared::commands::get_startup_time,
            shared::commands::get_local_ip_addresses,
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
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            init_logging(app.handle())?;

            let app_handle = app.handle();
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

            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service);

            tracing::info!("BedCode (Mobile) initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Pairing
            shared::commands::generate_pairing_code,
            shared::commands::get_current_pairing_code,
            shared::commands::verify_pairing_code,
            shared::commands::clear_pairing_code,
            shared::commands::list_paired_devices,
            shared::commands::remove_paired_device,
            // Quick Actions
            shared::commands::list_quick_actions,
            shared::commands::create_quick_action,
            shared::commands::update_quick_action,
            shared::commands::delete_quick_action,
            shared::commands::get_all_db_settings,
            shared::commands::set_db_setting,
            // Settings
            shared::commands::get_app_settings,
            shared::commands::save_app_settings,
            // Utility
            shared::commands::ping,
            shared::commands::get_app_version,
            shared::commands::get_local_ip_addresses,
            // Android Specific
            mobile::commands::get_status_bar_height,
            mobile::commands::set_screen_orientation,
            mobile::commands::keep_screen_awake,
            // Session Config
            shared::commands::list_session_configs,
            shared::commands::get_session_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}