//! BedCode - Library Entry Point

pub mod auth;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod notify;
pub mod parser;

// PTY and Session modules are desktop-only
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod pty;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod session;

// WebSocket server is desktop-only (mobile acts as client)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod websocket;

pub use error::{AppError, Result};

use auth::PairingService;
use auth::QrTokenManager;
use config::AppConfig;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 初始化日志系统
fn init_logging(app_handle: &tauri::AppHandle) -> Result<()> {
    // 获取日志目录
    let log_dir = app_handle
        .path()
        .app_log_dir()
        .expect("Failed to get log directory");

    // 确保日志目录存在
    std::fs::create_dir_all(&log_dir)?;

    // 创建日志文件
    let _log_file = log_dir.join("bedcode.log");
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("bedcode")
        .filename_suffix("log")
        .max_log_files(7) // 保留 7 天的日志
        .build(&log_dir)
        .expect("Failed to create log file appender");

    // 创建日志层
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true);

    // 初始化日志订阅者
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
fn insert_default_quick_actions(db: &db::Database) -> Result<()> {
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
        let mut action = db::QuickAction::new(name.to_string(), content.to_string());
        action.icon = Some(icon.to_string());
        action.color = Some(color.to_string());
        db.create_quick_action(&action)?;
    }

    tracing::info!("Inserted default quick actions");
    Ok(())
}

// ==================== 应用启动耗时记录 ====================

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

// ==================== Desktop Entry Point ====================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn run() {
    use session::SessionManager;
    use tauri::Emitter;
    use websocket::WebSocketServer;

    // 记录进程启动时间
    let app_start = AppStartTime(std::time::Instant::now());
    // Instant 实现了 Copy，此处复制一份供 setup 闭包内使用
    // app_start 在闭包内被 move 到 app.manage 后无法再访问
    let start = app_start.0;

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(move |app| {
            // 初始化日志系统
            init_logging(app.handle())?;

            // 将启动时间注入状态，供前端查询启动耗时
            app.manage(app_start);

            // 获取配置路径
            let app_handle = app.handle();
            let config_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("config.json");

            // 加载应用配置
            let app_config = AppConfig::load(&config_path).unwrap_or_else(|e| {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                AppConfig::default()
            });
            let ws_port = app_config.network.port;

            // Initialize database
            let db_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("bedcode.db");

            // Ensure parent directory exists
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Initialize database
            let db = db::Database::new(&db_path)?;
            db.init_schema()?;

            // Insert default quick actions if empty
            insert_default_quick_actions(&db)?;

            // Store database in app state
            let db = Arc::new(Mutex::new(db));
            app.manage(db.clone());

            // Initialize session manager
            let session_manager = Arc::new(SessionManager::new(db.clone()));
            app.manage(session_manager.clone());

            // Initialize pairing service
            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service.clone());

            // Initialize QR token manager
            let qr_manager = Arc::new(QrTokenManager::new());
            app.manage(qr_manager.clone());

            // Initialize and start WebSocket server
            let mut ws_server = WebSocketServer::new(
                ws_port,
                session_manager.clone(),
                db.clone(),
                pairing_service.clone(),
                qr_manager.clone(),
            );

            // Set app handle for event emission (wrap in Arc)
            use std::sync::Arc as StdArc;
            ws_server.set_app_handle(StdArc::new(app_handle.clone()));

            let ws_server = Arc::new(ws_server);

            // Store WebSocket server in app state for later access
            app.manage(ws_server.clone());

            // Start WebSocket server in background
            let ws_server_clone = ws_server.clone();
            tauri::async_runtime::spawn(async move {
                tracing::info!("Starting WebSocket server on port {}", ws_port);
                if let Err(e) = ws_server_clone.start().await {
                    tracing::error!("WebSocket server error: {}", e);
                }
            });

            // Start output event forwarder (forward PTY output to frontend)
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

            // Setup system tray
            setup_tray(app_handle)?;

            // 监听窗口关闭事件，清理资源
            let window = app_handle.get_webview_window("main").expect("Failed to get main window");
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    tracing::info!("Window close requested, shutting down...");
                    // SessionManager 的 Drop 会自动清理所有会话
                    // WebSocketServer 会在 app exit 时自动清理
                }
            });

            let init_elapsed = start.elapsed();
            tracing::info!("BedCode (Desktop) initialized - WebSocket server on port {} (后端初始化耗时: {}ms)", ws_port, init_elapsed.as_millis());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // WSL
            commands::list_wsl_distributions,
            commands::is_wsl_available,
            // Tmux
            commands::list_tmux_sessions,
            commands::is_tmux_available,
            commands::create_tmux_session,
            // Session Config
            commands::create_session_config,
            commands::list_session_configs,
            commands::get_session_config,
            commands::delete_session_config,
            commands::update_session_config,
            // Session
            commands::start_session,
            commands::list_sessions,
            commands::kill_session,
            commands::resize_session,
            // PTY Input
            commands::write_to_session,
            commands::send_special_key,
            // Pairing
            commands::generate_pairing_code,
            commands::get_current_pairing_code,
            commands::verify_pairing_code,
            commands::clear_pairing_code,
            commands::list_paired_devices,
            commands::remove_paired_device,
            // QR Code
            commands::generate_qr_code,
            commands::clear_qr_code,
            commands::get_qr_connection_info,
            commands::get_qr_token_ttl,
            commands::set_qr_token_ttl,
            // Quick Actions
            commands::list_quick_actions,
            commands::create_quick_action,
            commands::update_quick_action,
            commands::delete_quick_action,
            commands::get_terminal_history,
            commands::search_terminal_history,
            commands::get_all_db_settings,
            commands::set_db_setting,
            // Settings
            commands::get_app_settings,
            commands::save_app_settings,
            // Utility
            commands::ping,
            commands::get_app_version,
            commands::get_startup_time,
            commands::get_local_ip_addresses,
            commands::get_connected_devices,
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

    // Create menu items
    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide", "隐藏窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    // Create menu
    let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;

    // Create tray icon
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
            // 初始化日志系统
            init_logging(app.handle())?;

            // Initialize database
            let app_handle = app.handle();
            let db_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("bedcode.db");

            // Ensure parent directory exists
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Initialize database
            let db = db::Database::new(&db_path)?;
            db.init_schema()?;

            // Insert default quick actions if empty
            insert_default_quick_actions(&db)?;

            // Store database in app state
            let db = Arc::new(Mutex::new(db));
            app.manage(db.clone());

            // Initialize pairing service
            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service);

            tracing::info!("BedCode (Mobile) initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Pairing
            commands::generate_pairing_code,
            commands::get_current_pairing_code,
            commands::verify_pairing_code,
            commands::clear_pairing_code,
            commands::list_paired_devices,
            commands::remove_paired_device,
            // Quick Actions
            commands::list_quick_actions,
            commands::create_quick_action,
            commands::update_quick_action,
            commands::delete_quick_action,
            commands::get_terminal_history,
            commands::search_terminal_history,
            commands::get_all_db_settings,
            commands::set_db_setting,
            // Settings
            commands::get_app_settings,
            commands::save_app_settings,
            // Utility
            commands::ping,
            commands::get_app_version,
            commands::get_local_ip_addresses,
            // Android Specific
            commands::get_status_bar_height,
            commands::set_screen_orientation,
            commands::keep_screen_awake,
            // Session Config (for displaying saved configs)
            commands::list_session_configs,
            commands::get_session_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
