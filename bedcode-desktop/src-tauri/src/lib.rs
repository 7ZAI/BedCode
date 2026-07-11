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
///
/// 接受 LogConfig 参数，所有日志行为均可通过配置文件控制
fn init_logging(app_handle: &tauri::AppHandle, log_config: &system::config::LogConfig) -> Result<()> {
    let log_dir = app_handle
        .path()
        .app_log_dir()
        .expect("Failed to get log directory");

    std::fs::create_dir_all(&log_dir)?;

    // 解析轮转策略
    let rotation = match log_config.rotation.as_str() {
        "hourly" => tracing_appender::rolling::Rotation::HOURLY,
        "never" => tracing_appender::rolling::Rotation::NEVER,
        _ => tracing_appender::rolling::Rotation::DAILY,
    };

    // max_files: 0 表示不限制，不调用 .max_log_files() 让文件无限增长
    // tracing_appender 的 max_log_files 接受 usize，无"不限制"选项，只能通过不调用来实现

    // Error 日志文件：固定 ERROR 级别，始终记录最严重问题
    let mut error_builder = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(rotation.clone())
        .filename_prefix("error")
        .filename_suffix("log");
    if log_config.max_files > 0 {
        error_builder = error_builder.max_log_files(log_config.max_files);
    }
    let error_appender = error_builder
        .build(&log_dir)
        .expect("Failed to create error log file appender");

    // 运行时日志文件：级别由 log.file_level 控制
    let mut runtime_builder = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(rotation)
        .filename_prefix("runtime")
        .filename_suffix("log");
    if log_config.max_files > 0 {
        runtime_builder = runtime_builder.max_log_files(log_config.max_files);
    }
    let runtime_appender = runtime_builder
        .build(&log_dir)
        .expect("Failed to create runtime log file appender");

    // Error 日志层：固定 ERROR 及以上
    let error_layer = tracing_subscriber::fmt::layer()
        .with_writer(error_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_filter(EnvFilter::new("error"));

    // 运行时日志层：dev 构建强制 debug，release 使用配置值
    let file_level = if cfg!(debug_assertions) {
        "debug"
    } else {
        log_config.file_level.as_str()
    };
    let runtime_layer = tracing_subscriber::fmt::layer()
        .with_writer(runtime_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_filter(EnvFilter::new(file_level));

    // 控制台输出逻辑
    let should_add_console = cfg!(debug_assertions) || log_config.console_in_release;

    if should_add_console {
        // RUST_LOG 环境变量优先级最高，其次使用配置值
        let console_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&log_config.console_filter));
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
    } else {
        tracing_subscriber::registry()
            .with(error_layer)
            .with(runtime_layer)
            .try_init()
            .expect("Failed to set tracing subscriber");
    }

    tracing::info!("Logging initialized. Log directory: {:?}", log_dir);
    tracing::info!(
        "Log config: file_level={}, rotation={}, max_files={}, console_in_release={}",
        log_config.file_level, log_config.rotation, log_config.max_files, log_config.console_in_release,
    );
    tracing::info!("BedCode Desktop v{} starting...", env!("CARGO_PKG_VERSION"));

    Ok(())
}


/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

pub fn run() {
    use tauri::Emitter;

    let app_start = AppStartTime(std::time::Instant::now());
    let start = app_start.0;

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
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
                            Ok(_) => eprintln!("Default config copied from resource to {:?}", config_path),
                            Err(e) => eprintln!("Failed to copy default config: {}, using built-in defaults", e),
                        }
                    }
                }
            }

            // 先加载配置，再初始化日志系统，使日志行为可配置
            let app_config = crate::system::config::AppConfig::load(&config_path).unwrap_or_else(|e| {
                eprintln!("Failed to load config, using defaults: {}", e);
                crate::system::config::AppConfig::default()
            });

            // 初始化日志系统（依赖已加载的 LogConfig）
            init_logging(app.handle(), &app_config.log)?;

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

            // 设置 AppHandle 到 SessionManager
            // 会话创建时通过 subscribe_output() + FrontendOutputHandler::spawn() 转发输出
            // 替代旧的 AsyncPtyOutputListener + try_lock 模式，避免 PtyReader 同步线程中锁竞争丢数据
            tauri::async_runtime::block_on(async {
                session_manager.set_app_handle(app_handle.clone()).await;
            });
            tracing::info!("SessionManager app_handle configured for output forwarding");

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

            // ==================== 开发模式：启动插件文件监听 ====================
            // 仅 debug 构建启用，监听插件产物变化触发热重载
            #[cfg(debug_assertions)]
            {
                let _dev_watcher = plugin::watcher::PluginDevWatcher::start(plugins_dir.to_path_buf());
                // dev_watcher 需要 hold 住生命周期，存入 AppContext 或 leak
                // 使用 Box::leak 使 watcher 生命周期与进程一致（开发模式可接受）
                Box::leak(Box::new(_dev_watcher));
                tracing::info!("Plugin dev watcher enabled (debug build)");
            }

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
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // 检查生命周期钩子是否允许关闭（如存在运行中会话则阻止）
                    let should_close = tauri::async_runtime::block_on(async {
                        system::lifecycle::lifecycle_registry()
                            .run_window_close_hooks()
                            .await
                    });
                    if !should_close {
                        api.prevent_close();
                    }
                }
            });

            let init_elapsed = start.elapsed();
            tracing::info!("BedCode Desktop initialized - WebSocket server on port {} (后端初始化耗时: {}ms)", ws_port, init_elapsed.as_millis());

            // 注册核心模块的生命周期钩子（Shutdown/WindowClose）
            system::lifecycle::register_core_lifecycle_hooks();
            system::lifecycle::register_window_close_hooks();

            // 触发 Startup 钩子
            tauri::async_runtime::spawn(async move {
                system::lifecycle::lifecycle_registry().run_startup_hooks().await;
            });

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
            commands::plugin::plugin_dev_reload,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // 使用 .build() + .run() 替代 .run()，以接入 Tauri RunEvent 循环
    // RunEvent::ExitRequested 是执行优雅关闭的最后时机
    app.run(move |_app_handle, event| {
        match event {
            tauri::RunEvent::ExitRequested { .. } => {
                tracing::info!("BedCode Desktop exit requested, running shutdown hooks...");
                tauri::async_runtime::block_on(async {
                    system::lifecycle::lifecycle_registry()
                        .run_shutdown_hooks()
                        .await;
                });
            }
            tauri::RunEvent::Exit { .. } => {
                tracing::info!("BedCode Desktop exited");
            }
            _ => {}
        }
    });
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
