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
pub use shared::system;

// Re-export desktop modules for testing
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::session;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::pty;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::server;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::plugin;


#[cfg(not(any(target_os = "android", target_os = "ios")))]
use desktop::server::services::PairingService;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use desktop::server::port_checker;
#[cfg(any(target_os = "android", target_os = "ios"))]
use mobile::remote::PairingService;
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
use tracing_subscriber::Layer;
#[cfg(not(target_os = "android"))]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 初始化日志系统
#[allow(unused_variables)]
fn init_logging(_app_handle: &tauri::AppHandle) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        // Android: 只初始化 android_logger
        // tracing 的 "log" feature 会自动将 tracing:: 宏转发到 log::
        // android_logger 再将 log:: 输出发送到 adb logcat
        android_logger::init_once(
            Config::default()
                .with_max_level(LevelFilter::Debug)
                .with_tag("BedCode")
        );

        tracing::info!("BedCode Android logging initialized (tracing → log → logcat)");
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

    // Error 日志文件：只记录 ERROR 及以上级别
    let error_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("error")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)
        .expect("Failed to create error log file appender");

    // 运行时日志文件：记录 INFO 及以上级别
    let runtime_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("runtime")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)
        .expect("Failed to create runtime log file appender");

    // Error 日志层：只接收 ERROR 及以上
    let error_layer = tracing_subscriber::fmt::layer()
        .with_writer(error_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_filter(tracing_subscriber::filter::LevelFilter::ERROR);

    // 运行时日志层：INFO 及以上
    let runtime_layer = tracing_subscriber::fmt::layer()
        .with_writer(runtime_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

    #[cfg(debug_assertions)]
    {
        // Debug 模式：控制台输出 DEBUG 及以上
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true)
            .with_target(true)
            .pretty()
            .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG);

        tracing_subscriber::registry()
            .with(error_layer)
            .with(runtime_layer)
            .with(console_layer)
            .init();
    }

    #[cfg(not(debug_assertions))]
    {
        tracing_subscriber::registry()
            .with(error_layer)
            .with(runtime_layer)
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

            // 首次启动时从打包资源复制默认配置到 AppData
            // 后续启动直接使用 AppData 中的配置，用户修改不会丢失
            if !config_path.exists() {
                if let Ok(resource_path) = app_handle
                    .path()
                    .resolve("resources/config.json", tauri::path::BaseDirectory::Resource)
                {
                    if resource_path.exists() {
                        if let Some(parent) = config_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::copy(&resource_path, &config_path) {
                            Ok(_) => tracing::info!("Default config copied from resource to {:?}", config_path),
                            Err(e) => tracing::warn!("Failed to copy default config: {}, using built-in defaults", e),
                        }
                    }
                }
            }

            let app_config = crate::shared::system::config::AppConfig::load(&config_path).unwrap_or_else(|e| {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                crate::shared::system::config::AppConfig::default()
            });

            // 初始化全局配置单例
            crate::shared::system::config::AppConfig::init(app_config.clone());

            let mut app_config = app_config;

            // Token 校验/生成：确保 plugin token 合法
            let token_result = crate::desktop::plugin::setup::ensure_token(
                &mut app_config,
                &config_path,
            );
            if token_result.token_generated {
                // 配置可能修改了 token，重新初始化全局配置
                crate::shared::system::config::AppConfig::init(app_config.clone());
            }

            // 清理旧版全局 hooks（迁移到项目级后不再需要全局 hooks）
            crate::desktop::plugin::setup::cleanup_global_hooks();

            // 保存 resource_dir 供后续会话创建时使用
            let resource_dir = app_handle
                .path()
                .resource_dir()
                .expect("Failed to get resource dir");

            let ws_port = app_config.network.port;

            // 检查端口可用性
            let ws_port = match port_checker::check_and_resolve_port(&app_handle, ws_port) {
                Ok(port) => port,
                Err(e) => {
                    tracing::error!("Port check failed: {}", e);
                    ws_port // 使用原端口，服务器启动会失败并记录日志
                }
            };

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

            // ==================== 创建所有全局单实例 ====================

            let storage = Arc::new(desktop::session::SessionStorage::new(db.clone()));
            let resource_dir_arc = Arc::new(resource_dir);
            let session_manager = Arc::new(desktop::session::SessionManager::new(storage, resource_dir_arc.clone()));
            let config_manager = Arc::new(desktop::session::SessionConfigManager::new(db.clone()));
            let plugin_manager = Arc::new(desktop::plugin::PluginManager::new());
            let pairing_service = Arc::new(PairingService::new());
            let qr_manager = Arc::new(crate::desktop::auth::QrTokenManager::new());
            let app_handle_arc = Arc::new(app_handle.clone());

            // 创建同步事件通道
            let (sync_tx, _) = tokio::sync::broadcast::channel::<desktop::events::DesktopSyncEvent>(64);

            // 设置 SessionManager 和 SessionConfigManager 的同步事件发送器
            tauri::async_runtime::block_on(async {
                session_manager.set_sync_tx(sync_tx.clone()).await;
                config_manager.set_sync_tx(sync_tx.clone()).await;
            });

            // 创建并同步设置 PTY 输出监听器
            // 必须在 setup 返回前完成，否则会话启动时监听器可能未就绪导致输出丢失
            let frontend_handler = Arc::new(desktop::pty::FrontendOutputHandler::new(app_handle.clone()));
            let async_listener = Arc::new(desktop::pty::AsyncPtyOutputListener::new());
            tauri::async_runtime::block_on(async {
                async_listener.register(frontend_handler).await;
                session_manager.set_output_listener(async_listener).await;
            });
            tracing::info!("PTY output listener configured (frontend)");

            // ==================== 注册到 AppContext 全局容器 ====================

            let ctx = desktop::app_context::AppContextBuilder::new()
                .db(db.clone())
                .session_manager(session_manager.clone())
                .config_manager(config_manager.clone())
                .plugin_manager(plugin_manager.clone())
                .pairing_service(pairing_service.clone())
                .qr_manager(qr_manager.clone())
                .app_handle(app_handle_arc.clone())
                .sync_tx(sync_tx.clone())
                .resource_dir(resource_dir_arc.clone())
                .build_and_init();

            // 同时注册到 Tauri State（前端 invoke 可用）
            app.manage(db.clone());
            app.manage(config_manager.clone());
            app.manage(session_manager.clone());
            app.manage(pairing_service.clone());
            app.manage(qr_manager.clone());

            // ==================== 启动 WebSocket 服务 ====================

            let ws_manager = desktop::websocket_manager::WebSocketManager::global();
            let ws_port_for_spawn = ws_port;
            tauri::async_runtime::spawn(async move {
                ws_manager.init().await
                    .expect("Failed to initialize WebSocketManager");

                // 注册同步事件处理器
                use crate::shared::event::handler::EventHandler;
                use crate::shared::event::global_matcher;
                use crate::desktop::events::{DesktopSyncEvent, SyncEventHandler};

                // 注册事件源
                global_matcher().register_source::<DesktopSyncEvent>(ctx.sync_tx().clone()).await;

                // 注册处理器
                let sync_handler = Arc::new(SyncEventHandler::new(
                    ctx.session_manager().clone(),
                    ctx.config_manager().clone(),
                    ws_manager,
                ));
                global_matcher().register::<DesktopSyncEvent>(sync_handler).await;
                tracing::info!("[BedCode] SyncEventHandler registered");

                // 注册 WebSocket 服务器事件处理器（已移至 Actix WS actor 内部）

                tracing::info!("[BedCode] Starting WebSocket server on port {}", ws_port_for_spawn);
                match ws_manager.start(ws_port_for_spawn).await {
                    Ok(_) => {
                        tracing::info!("[BedCode] WebSocket server started successfully");

                        // Actix Web server 已启动（HTTP + WS 统一端口）
                        tracing::info!("[BedCode] Actix Web server started");
                    }
                    Err(e) => tracing::error!("[BedCode] WebSocket server failed to start: {}", e),
                }
            });

            // 写入端口文件
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

            // 启动事件转发器：将 SessionManager 的事件转发到前端
            let event_forwarder = desktop::EventForwarder::new(
                app_handle.clone(),
                session_manager.clone(),
            );
            event_forwarder.start();

            setup_tray(app_handle)?;

            let window = app_handle.get_webview_window("main").expect("Failed to get main window");
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    tracing::info!("Window close requested, shutting down...");
                }
            });

            let init_elapsed = start.elapsed();
            tracing::info!("BedCode (Desktop) initialized - WebSocket server on port {} (后端初始化耗时: {}ms)", ws_port, init_elapsed.as_millis());

            // 发送 Token 配置结果到前端
            let app_handle_for_plugin = app_handle_arc.clone();
            tauri::async_runtime::spawn(async move {
                // 延迟 500ms 发送，确保前端已加载完成
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = app_handle_for_plugin.emit("plugin-setup-result", &token_result);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // WSL
            desktop::commands::wsl::list_wsl_distributions,
            desktop::commands::wsl::is_wsl_available,
            // Session Config
            desktop::commands::session_config::create_session_config,
            desktop::commands::session_config::list_session_configs,
            desktop::commands::session_config::get_session_config,
            desktop::commands::session_config::delete_session_config,
            desktop::commands::session_config::update_session_config,
            // Session
            desktop::commands::session::start_session,
            desktop::commands::session::create_session_no_start,
            desktop::commands::session::start_existing_session,
            desktop::commands::session::list_sessions,
            desktop::commands::session::get_session,
            desktop::commands::session::kill_session,
            desktop::commands::session::delete_session,
            desktop::commands::session::restart_session,
            desktop::commands::session::resize_session,
            desktop::commands::session::get_session_output_history,
            // PTY Input
            desktop::commands::pty_input::write_to_session,
            desktop::commands::pty_input::send_special_key,
            // Pairing
            shared::system::commands::generate_pairing_code,
            shared::system::commands::get_current_pairing_code,
            shared::system::commands::verify_pairing_code,
            shared::system::commands::clear_pairing_code,
            shared::system::commands::list_paired_devices,
            shared::system::commands::remove_paired_device,
            // QR Code
            desktop::commands::qr::generate_qr_code,
            desktop::commands::qr::clear_qr_code,
            desktop::commands::qr::get_qr_connection_info,
            desktop::commands::qr::get_qr_token_ttl,
            desktop::commands::qr::set_qr_token_ttl,
            // Quick Actions
            desktop::commands::quick_actions::list_quick_actions,
            desktop::commands::quick_actions::create_quick_action,
            desktop::commands::quick_actions::update_quick_action,
            desktop::commands::quick_actions::delete_quick_action,
            desktop::commands::settings::get_all_db_settings,
            desktop::commands::settings::set_db_setting,
            // Settings
            shared::system::commands::get_app_settings,
            shared::system::commands::save_app_settings,
            // Utility
            shared::system::commands::ping,
            shared::system::commands::get_app_version,
            shared::system::commands::get_startup_time,
            shared::system::commands::get_local_ip_addresses,
            desktop::commands::devices::get_connected_devices,
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
            // 日志已在 run() 中早期初始化，这里不再重复初始化
            tracing::info!("BedCode setup starting...");

            // 插件初始化日志
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

    // 这行永远不会执行，因为 run() 会阻塞
    tracing::info!("BedCode application closed");
}