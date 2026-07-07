//! BedCode Desktop - Library Entry Point

// ==================== Domain Modules ====================

pub mod commands;
pub mod db;
pub mod enums;
pub mod events;
pub mod mdns;
pub mod plugin;
pub mod process;
pub mod pty;
pub mod server;
pub mod session;
pub mod system;
pub mod utils;

// ==================== Re-exports ====================

pub use system::{AppError, Result, AppConfig, AppContext};

// ==================== Application Setup ====================

use db::Database;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;
use tracing_subscriber::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化日志系统
fn init_logging(app_handle: &tauri::AppHandle) -> Result<()> {
    init_logging_desktop(app_handle)
}

fn init_logging_desktop(app_handle: &tauri::AppHandle) -> Result<()> {
    // LogTracer 桥接由 try_init() 自动处理，无需手动初始化

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
        .with_filter(EnvFilter::new("error"));

    // 运行时日志层：INFO 及以上
    let runtime_layer = tracing_subscriber::fmt::layer()
        .with_writer(runtime_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_filter(EnvFilter::new("info"));

    #[cfg(debug_assertions)]
    {
        // Debug 模式：控制台输出，支持 RUST_LOG 环境变量动态控制
        // 默认 bedcode_lib=debug,actix_web=info；可通过 RUST_LOG 覆盖
        let console_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("bedcode_lib=debug,actix_web=info,actix_http=info"));
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true)
            .with_target(true)
            .pretty()
            .with_filter(console_filter);

        tracing_subscriber::registry()
            .with(error_layer)
            .with(runtime_layer)
            .with(console_layer)
            .try_init()
            .expect("Failed to set tracing subscriber");
    }

    #[cfg(not(debug_assertions))]
    {
        tracing_subscriber::registry()
            .with(error_layer)
            .with(runtime_layer)
            .try_init()
            .expect("Failed to set tracing subscriber");
    }

    tracing::info!("Logging initialized. Log directory: {:?}", log_dir);
    tracing::info!("BedCode Desktop v{} starting...", env!("CARGO_PKG_VERSION"));

    Ok(())
}


/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

pub fn run() {
    use tauri::Emitter;

    let app_start = AppStartTime(std::time::Instant::now());
    let start = app_start.0;

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            init_logging(app.handle())?;
            app.manage(app_start);

            let app_handle = app.handle();
            let config_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("config.properties");

            // 首次启动时从打包资源复制默认配置到 AppData
            // 后续启动直接使用 AppData 中的配置，用户修改不会丢失
            if !config_path.exists() {
                if let Ok(resource_path) = app_handle
                    .path()
                    .resolve("resources/config.properties", tauri::path::BaseDirectory::Resource)
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

            let app_config = crate::system::config::AppConfig::load(&config_path).unwrap_or_else(|e| {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                crate::system::config::AppConfig::default()
            });

            // 初始化全局配置单例
            crate::system::config::AppConfig::init(app_config.clone());

            // 同步 PowerManager 开关状态到配置值
            crate::system::power::power_manager().set_enabled(app_config.network.prevent_sleep);

            let mut app_config = app_config;

            // Token 校验/生成：确保 plugin token 合法
            let token_result = crate::plugin::setup::ensure_token(
                &mut app_config,
                &config_path,
            );
            if token_result.token_generated {
                // 配置可能修改了 token，重新初始化全局配置
                crate::system::config::AppConfig::init(app_config.clone());
            }

            // 清理旧版全局 hooks（迁移到项目级后不再需要全局 hooks）
            crate::plugin::setup::cleanup_global_hooks();

            // 保存 resource_dir 供后续会话创建时使用
            let resource_dir = app_handle
                .path()
                .resource_dir()
                .expect("Failed to get resource dir");

            // 解析桌面端插件目录
            // dev 模式下 resolve 指向 target/debug/resources/...（Tauri 不自动复制资源）
            // 生产模式下 resolve 指向安装目录的 resources/...（打包时已包含）
            // 因此 dev 模式回退到源码目录
            let plugins_dir = {
                let resolved = app_handle
                    .path()
                    .resolve("resources/plugins/desktop", tauri::path::BaseDirectory::Resource)
                    .expect("Failed to resolve plugins directory");
                if resolved.exists() {
                    resolved
                } else {
                    // dev 模式 fallback：使用源码目录
                    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
                        .expect("CARGO_MANIFEST_DIR not set");
                    let fallback = std::path::PathBuf::from(manifest_dir)
                        .join("resources").join("plugins").join("desktop");
                    tracing::info!("Plugin resolved path not found, falling back to source dir: {:?}", fallback);
                    fallback
                }
            };

            let ws_port = app_config.network.port;

            // 检查端口可用性
            let ws_port = match server::port_checker::check_and_resolve_port(&app_handle, ws_port) {
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
           

            let db = Arc::new(Mutex::new(db));

            // ==================== 创建所有全局单实例 ====================

            let storage = Arc::new(session::SessionStorage::new(db.clone()));
            let resource_dir_arc = Arc::new(resource_dir);
            let session_manager = Arc::new(session::SessionManager::new(storage, resource_dir_arc.clone()));
            let config_manager = Arc::new(session::SessionConfigManager::new(db.clone()));
            let plugin_manager = Arc::new(plugin::PluginManager::new());
            // app_handle_arc 需在 plugin_host 之前创建，因为 PluginHost::new() 需要它构建 HostContextFns
            let app_handle_arc = Arc::new(app_handle.clone());
            let plugin_host = Arc::new(
                tauri::async_runtime::block_on(
                    plugin::PluginHost::new(db.clone(), &plugins_dir, session_manager.clone(), app_handle_arc.clone())
                )
            );
            let pairing_service = Arc::new(server::services::pairing_service::PairingService::new());
            let qr_manager = Arc::new(utils::auth::QrTokenManager::new());
            let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(mdns::advertiser::MdnsAdvertiser::new()));

            // 创建同步事件通道
            let (sync_tx, _) = tokio::sync::broadcast::channel::<events::DesktopSyncEvent>(64);

            // 设置 SessionManager 和 SessionConfigManager 的同步事件发送器
            tauri::async_runtime::block_on(async {
                session_manager.set_sync_tx(sync_tx.clone()).await;
                config_manager.set_sync_tx(sync_tx.clone()).await;
            });

            // 创建并同步设置 PTY 输出监听器
            // 必须在 setup 返回前完成，否则会话启动时监听器可能未就绪导致输出丢失
            let frontend_handler = Arc::new(pty::FrontendOutputHandler::new(app_handle.clone()));
            let async_listener = Arc::new(pty::AsyncPtyOutputListener::new());
            tauri::async_runtime::block_on(async {
                async_listener.register(frontend_handler).await;
                session_manager.set_output_listener(async_listener).await;
            });
            tracing::info!("PTY output listener configured (frontend)");

            // ==================== 注册到 AppContext 全局容器 ====================

            let ctx = system::app_context::AppContextBuilder::new()
                .db(db.clone())
                .session_manager(session_manager.clone())
                .config_manager(config_manager.clone())
                .plugin_manager(plugin_manager.clone())
                .plugin_host(plugin_host.clone())
                .pairing_service(pairing_service.clone())
                .qr_manager(qr_manager.clone())
                .mdns_advertiser(mdns_advertiser.clone())
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
            app.manage(mdns_advertiser.clone());
            app.manage(plugin_host.clone());

            // ==================== 启动服务器（通过 ServerSupervisor）====================

            let supervisor = server::supervisor::ServerSupervisor::global();
            let ws_port_for_spawn = ws_port;
            let auto_start = app_config.network.auto_start;
            tauri::async_runtime::spawn(async move {
                supervisor.init_config(ws_port_for_spawn, auto_start).await;

                // 注册同步事件处理器
                use crate::events::global_matcher;
                use crate::events::{DesktopSyncEvent, SyncEventHandler};

                let ws_manager = crate::server::ws::WebSocketManager::global();
                ws_manager.init().await.expect("Failed to initialize WebSocketManager");

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

                if auto_start {
                    tracing::info!("[BedCode] Auto-starting server on port {}", ws_port_for_spawn);
                    match supervisor.start(ws_port_for_spawn).await {
                        Ok(_) => tracing::info!("[BedCode] Server started successfully"),
                        Err(e) => tracing::error!("[BedCode] Server failed to start: {}", e),
                    }
                } else {
                    tracing::info!("[BedCode] Server auto-start disabled, waiting for manual start");
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
            let event_forwarder = events::EventForwarder::new(
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
            tracing::info!("BedCode Desktop initialized - WebSocket server on port {} (后端初始化耗时: {}ms)", ws_port, init_elapsed.as_millis());

            // TODO: 插件功能暂未上线，不再发送 Token 配置结果 toast
            // let app_handle_for_plugin = app_handle_arc.clone();
            // tauri::async_runtime::spawn(async move {
            //     // 延迟 500ms 发送，确保前端已加载完成
            //     tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            //     let _ = app_handle_for_plugin.emit("plugin-setup-result", &token_result);
            // });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // WSL
            commands::wsl::list_wsl_distributions,
            commands::wsl::is_wsl_available,
            // Session Config
            commands::session_config::create_session_config,
            commands::session_config::list_session_configs,
            commands::session_config::get_session_config,
            commands::session_config::delete_session_config,
            commands::session_config::update_session_config,
            // Session
            commands::session::start_session,
            commands::session::create_session_no_start,
            commands::session::start_existing_session,
            commands::session::list_sessions,
            commands::session::get_session,
            commands::session::kill_session,
            commands::session::delete_session,
            commands::session::restart_session,
            commands::session::resize_session,
            commands::session::get_session_output_history,
            // PTY Input
            commands::pty_input::write_to_session,
            commands::pty_input::send_special_key,
            // Pairing
            commands::system::generate_pairing_code,
            commands::system::get_current_pairing_code,
            commands::system::verify_pairing_code,
            commands::system::clear_pairing_code,
            commands::system::list_paired_devices,
            commands::system::remove_paired_device,
            // QR Code
            commands::qr::generate_qr_code,
            commands::qr::clear_qr_code,
            commands::qr::get_qr_connection_info,
            commands::qr::get_qr_token_ttl,
            commands::qr::set_qr_token_ttl,

            commands::settings::get_all_db_settings,
            commands::settings::set_db_setting,
            // Settings
            commands::system::get_app_settings,
            commands::system::save_app_settings,
            // Utility
            commands::system::ping,
            commands::system::get_app_version,
            commands::system::get_startup_time,
            commands::system::get_local_ip_addresses,
            commands::devices::get_connected_devices,
            // Plugin
            commands::plugin::plugin_list_loaded,
            commands::plugin::plugin_get_info,
            commands::plugin::plugin_activate,
            commands::plugin::plugin_deactivate,
            commands::plugin::plugin_mark_error,
            commands::plugin::plugin_storage_get,
            commands::plugin::plugin_storage_set,
            commands::plugin::plugin_storage_delete,
            commands::plugin::plugin_terminal_send_input,
            commands::plugin::plugin_list_commands,
            commands::plugin::plugin_list_views,
            commands::plugin::plugin_find_file_handler,
            commands::plugin::plugin_invoke,
            commands::plugin::plugin_list_rust_commands,
            // Server
            commands::server::server_start,
            commands::server::server_stop,
            commands::server::server_restart,
            commands::server::get_server_status,
            commands::server::get_server_metrics,
            commands::server::get_server_network_config,
            commands::server::update_server_port,
            commands::server::update_server_auto_start,
            commands::server::update_server_network_config,
            commands::server::reset_server_network_config,
            // mDNS
            commands::mdns::mdns_start_advertise,
            commands::mdns::mdns_stop_advertise,
            commands::mdns::mdns_is_advertising,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Setup system tray
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
