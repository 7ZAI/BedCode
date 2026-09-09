//! BedCode Desktop - Library Entry Point

// ==================== Domain Modules ====================

pub mod commands;
pub mod db;
pub mod enums;
pub mod events;
pub mod mdns;
pub mod peer_migration;
pub mod peer_net;
pub mod peer_receive;
pub mod peer_remote;
pub mod peer_transfer;
pub mod plugin;
pub mod process;
pub mod pty;
pub mod server;
pub mod session;
pub mod system;
pub mod utils;

// ==================== Re-exports ====================

use commands::system::RunningSessionInfo;
use system::constants::network::SYNC_EVENT_BROADCAST_CAPACITY;
pub use system::{AppConfig, AppContext, AppError, Result};

// ==================== Application Setup ====================

use db::Database;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

/// 删除当天已存在的日志文件（仅 dev 构建调用）
///
/// dev 启动频次高，按天追加会让同一天的日志混入多次启动的片段，难以定位；
/// 因此 dev 启动时替换当天日志（删旧建新）。release 保持按天追加轮转。
/// 必须在 RollingFileAppender 构建前调用，确保 appender 首次写入创建全新文件。
#[cfg(debug_assertions)]
fn reset_today_logs(log_dir: &std::path::Path) {
    // tracing_appender 的 rolling 文件名日期用 UTC（与本地日期可能错位一天）
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    // 日志重置列表：runtime / error / frontend（前端 console 单独文件，见 init_logging）
    for prefix in ["runtime", "error", "frontend"] {
        let path = log_dir.join(format!("{prefix}.{today}.log"));
        if path.exists() {
            match std::fs::remove_file(&path) {
                Ok(()) => eprintln!("[logging] dev reset: replaced today's log {}", path.display()),
                Err(e) => eprintln!("[logging] dev reset: failed to replace {}: {}", path.display(), e),
            }
        }
    }
}

/// 初始化日志系统
///
/// 接受 LogConfig 参数，所有日志行为均可通过配置文件控制
fn init_logging(app_handle: &tauri::AppHandle, log_config: &system::config::LogConfig) -> Result<()> {
    let log_dir = app_handle.path().app_log_dir().expect("Failed to get log directory");

    std::fs::create_dir_all(&log_dir)?;

    // dev 构建替换当天日志（release 保持追加轮转）
    #[cfg(debug_assertions)]
    reset_today_logs(&log_dir);

    // 2026-08: 字节 dump 目录记录已注释禁用（临时调试，恢复排查时取消注释）
    // system::logging::set_dump_dir(log_dir.clone());

    // 构建日志订阅器：文件层非阻塞写盘 + 控制台层。
    // 过滤语义与旧实现一致（error 固定 ERROR、runtime 按级别、frontend 仅 dev），
    // 全部收敛于 system::logging::build_logging，便于独立单测（见该模块测试）
    let (setup, subscriber) =
        system::logging::build_logging(&log_dir, log_config, cfg!(debug_assertions))?;
    install_subscriber(subscriber);

    // 保存句柄到进程级全局：worker guard 存活到进程退出（drop 时 flush 剩余日志）；
    // 级别热调（file_level_reload）与容量裁剪/丢弃告警（writers）由后续模块从此读取
    system::logging::store_setup(setup);

    // 启动后台日志维护任务：容量裁剪（超限删最旧，当前在写文件除外）+ 非阻塞队列丢弃告警（03）
    system::logging::spawn_log_maintenance(
        system::logging::global_setup().expect("logging setup stored before maintenance"),
        log_config.capacity_bytes,
    );

    tracing::info!("Logging initialized. Log directory: {:?}", log_dir);
    tracing::info!(
        "Log config: file_level={}, rotation={}, max_files={}, console_in_release={}",
        log_config.file_level,
        log_config.rotation,
        log_config.max_files,
        log_config.console_in_release,
    );
    tracing::info!("BedCode Desktop v{} starting...", env!("CARGO_PKG_VERSION"));

    Ok(())
}

/// 安装 tracing 全局订阅器（容忍 log→tracing 桥已被抢占）
///
/// debug 构建下 tauri-plugin-wdio 的 `.setup()` 会先 `log::set_boxed_logger`
/// 抢占 log 全局 logger（其 setup 早于应用 setup 执行），使 `try_init()` 在
/// `LogTracer::init()` 阶段返回 `SetLoggerError` 并 panic。拆成两步：桥安装
/// 失败仅忽略（log 记录由 wdio 自带 logger 承接），tracing 全局默认仍须设置
/// （与 `try_init` 第二步 `set_global_default` 等价）。
fn install_subscriber<S>(subscriber: S)
where
    S: tracing::Subscriber + Send + Sync + 'static,
{
    let _ = tracing_log::LogTracer::init();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

pub fn run() {
    use tauri::Emitter;

    let app_start = AppStartTime(std::time::Instant::now());
    let start = app_start.0;

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    // WDIO 测试插件仅 debug 构建注册（release 不编译该依赖、不注册该插件）
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_wdio::init());
    }

    let app = builder.setup(move |app| {
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

            // 启动电源唤醒监听：Windows 显示器长时间熄灭/锁屏后，WebView2 可能黑屏且不自愈，
            // 系统唤醒时强制窗口重绘（详见 system::power_wake 模块文档）
            crate::system::power_wake::spawn_wake_monitor(app_handle.clone());

            // 保存 resource_dir 供后续会话创建时使用
            let resource_dir = app_handle.path().resource_dir().expect("Failed to get resource dir");

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
                    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
                    let fallback = std::path::PathBuf::from(manifest_dir)
                        .join("resources")
                        .join("plugins")
                        .join("desktop");
                    tracing::info!(
                        "Plugin resolved path not found, falling back to source dir: {:?}",
                        fallback
                    );
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

            // 对等网络节点身份：目录与 DB 同源解析自 app_data_dir（决策 D3 宿主只
            // 注入目录），node_identity.json 与 bedcode.db 并列存放；错误经 ? 上抛
            // 走既有启动失败路径——静默换身份会让对端可信列表全部失效（D3）
            let peer_net_data_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            crate::peer_net::init_node_identity(&peer_net_data_dir)?;

            // 对等网络节点状态容器（ticket 03）：节点生命周期随文件传输插件
            // 启停（插件管理器 activate/deactivate 外壳接线，
            // 见 peer_net::ensure_node_started；旧 setup 无条件自启已退役）
            app.manage(crate::peer_net::PeerNetState::default());
            app.manage(crate::peer_transfer::PeerTransferState::default());
            app.manage(crate::peer_receive::PeerReceiveState::default());
            app.manage(crate::peer_remote::PeerRemoteState::default());

            let db = Database::new(&db_path)?;
            db.init_schema()?;

            let db = Arc::new(Mutex::new(db));

            // 旧版对等网络数据一次性迁移（issue 13 Phase 4 步骤 9；幂等，失败不阻断）
            crate::peer_migration::migrate_legacy_peer_data(&app_handle, &db);

            // ==================== 创建所有全局单实例 ====================

            // 采集系统基本信息（OS / 设备名称 / IP），挂载到 AppContext 供全局引用
            let system_info = Arc::new(system::info::SystemInfo::collect());

            let storage = Arc::new(session::SessionStorage::new(db.clone()));
            let resource_dir_arc = Arc::new(resource_dir);
            let session_manager = Arc::new(session::SessionManager::new(storage, resource_dir_arc.clone()));
            let config_manager = Arc::new(session::SessionConfigManager::new(db.clone()));
            // app_handle_arc 需在 plugin_host 之前创建，因为 PluginHost::new() 需要它构建 HostContextFns
            let app_handle_arc = Arc::new(app_handle.clone());
            let plugin_host = Arc::new(tauri::async_runtime::block_on(plugin::PluginHost::new(
                db.clone(),
                &plugins_dir,
                session_manager.clone(),
                config_manager.clone(),
                Some(app_handle_arc.clone()),
            )));
            // 注入消息总线 dispatcher（两阶段初始化）
            tauri::async_runtime::block_on(plugin_host.init_message_bus());
            let pairing_service = Arc::new(server::services::pairing_service::PairingService::new());
            let qr_manager = Arc::new(utils::auth::QrTokenManager::new());
            let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(mdns::advertiser::MdnsAdvertiser::new()));

            // 创建同步事件通道
            let (sync_tx, _) =
                tokio::sync::broadcast::channel::<events::DesktopSyncEvent>(SYNC_EVENT_BROADCAST_CAPACITY);

            // 设置 SessionManager 和 SessionConfigManager 的同步事件发送器
            tauri::async_runtime::block_on(async {
                session_manager.set_sync_tx(sync_tx.clone()).await;
                config_manager.set_sync_tx(sync_tx.clone()).await;
            });

            // ==================== 注册到 AppContext 全局容器 ====================

            let ctx = system::app_context::AppContextBuilder::new()
                .db(db.clone())
                .session_manager(session_manager.clone())
                .config_manager(config_manager.clone())
                .plugin_host(plugin_host.clone())
                .pairing_service(pairing_service.clone())
                .qr_manager(qr_manager.clone())
                .mdns_advertiser(mdns_advertiser.clone())
                .app_handle(Some(app_handle_arc.clone()))
                .sync_tx(sync_tx.clone())
                .resource_dir(resource_dir_arc.clone())
                .system_info(system_info.clone())
                .build_and_init();

            // 同时注册到 Tauri State（前端 invoke 可用）
            app.manage(db.clone());
            app.manage(config_manager.clone());
            app.manage(session_manager.clone());
            app.manage(pairing_service.clone());
            app.manage(qr_manager.clone());
            app.manage(mdns_advertiser.clone());
            app.manage(plugin_host.clone());
            app.manage(plugin_host.wasm_runtime().fs_auth().clone());
            app.manage(system_info.clone());

            // peer-net 节点与文件传输插件状态对账：boot 装配期 AppContext 全局
            // 尚未注册，activate 外壳内的节点启动会被静默跳过（2026-09-06 实机
            // 实证：已激活插件的节点不随 boot 启动，需手动开关插件才广播）——
            // 装配完成后按最终插件状态补对账
            if let Err(e) = tauri::async_runtime::block_on(
                crate::peer_net::sync_node_with_plugin_state(&app_handle_arc),
            ) {
                tracing::error!(error = %e, "peer-net node sync after boot assembly failed");
            }

            // ==================== 开发模式：启动插件文件监听 ====================
            // 仅 debug 构建启用，监听插件产物变化触发热重载
            // notify 回调在非 Tokio 线程中运行，必须通过 Handle::spawn 而非 tokio::spawn
            // setup 闭包不在 Tokio runtime 上下文中，需通过 block_on 获取 Handle
            #[cfg(debug_assertions)]
            {
                let runtime_handle = tauri::async_runtime::block_on(async { tokio::runtime::Handle::current() });
                let _dev_watcher = plugin::watcher::PluginDevWatcher::start(plugins_dir.to_path_buf(), runtime_handle);
                // dev_watcher 需要 hold 住生命周期，存入 AppContext 或 leak
                // 使用 Box::leak 使 watcher 生命周期与进程一致（开发模式可接受）
                Box::leak(Box::new(_dev_watcher));
                tracing::info!("Plugin dev watcher enabled (debug build)");
            }

            // ==================== 启动服务器（通过 ServerSupervisor）====================

            // 链路加密装配句柄（issue 01）：init_at_startup 需访问数据目录与 DB 状态
            let link_crypto_app_handle = app_handle.clone();
            let supervisor = server::supervisor::ServerSupervisor::global();
            let ws_port_for_spawn = ws_port;
            // 产品决策：服务器永久自启动，不再可配置（本地功能依赖此服务，
            // 见 ServerSupervisor 类注释；config 中 network.auto_start 已废弃）
            let auto_start = true;
            tauri::async_runtime::spawn(async move {
                // 链路加密先于服务器启动装配：第一条流量就要被开关裁决（spec §6）；
                // 身份损坏时强制回退全关，不阻断启动
                server::link_crypto::init_at_startup(&link_crypto_app_handle).await;

                supervisor.init_config(ws_port_for_spawn, auto_start).await;

                // 注册同步事件处理器
                use crate::events::global_matcher;
                use crate::events::{DesktopSyncEvent, SyncEventHandler};

                let ws_manager = crate::server::ws::WebSocketManager::global();
                ws_manager.init().await.expect("Failed to initialize WebSocketManager");

                // 注册事件源
                global_matcher()
                    .register_source::<DesktopSyncEvent>(ctx.sync_tx().clone())
                    .await;

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
            let event_forwarder = events::EventForwarder::new(app_handle.clone(), session_manager.clone());
            event_forwarder.start();

            setup_tray(app_handle)?;

            let window = app_handle
                .get_webview_window("main")
                .expect("Failed to get main window");
            let close_window = window.clone();
            let close_app_handle = app_handle.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // 始终先阻止默认关闭，避免 block_on 死锁
                    // 在同步回调中使用 block_on 会在 Tokio 运行时繁忙时死锁，
                    // 因此改为先阻止关闭，再 spawn 异步任务检查钩子
                    api.prevent_close();

                    let win = close_window.clone();
                    let ah = close_app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let should_close = system::lifecycle::lifecycle_registry().run_window_close_hooks().await;

                        if should_close {
                            // 无运行中会话，直接关闭
                            if let Err(e) = win.destroy() {
                                tracing::error!("Failed to destroy window: {}", e);
                            }
                        } else {
                            // 有运行中会话，通知前端弹窗确认
                            let ctx = system::app_context::AppContext::global();
                            let sm = ctx.session_manager();
                            let sessions = sm.list_sessions().await;
                            let running: Vec<_> = sessions
                                .iter()
                                .filter(|s| {
                                    matches!(
                                        s.status,
                                        enums::SessionStatus::Running
                                            | enums::SessionStatus::Starting
                                            | enums::SessionStatus::WaitingInput
                                    )
                                })
                                .map(|s| RunningSessionInfo {
                                    id: s.id.clone(),
                                    name: s.name.clone(),
                                    status: format!("{:?}", s.status),
                                })
                                .collect();

                            tracing::info!(
                                "Window close requested with {} running session(s), emitting to frontend",
                                running.len()
                            );

                            if let Err(e) = ah.emit(system::constants::event::WINDOW_CLOSE_REQUESTED, &running) {
                                tracing::error!("Failed to emit window-close-requested: {}", e);
                            }
                        }
                    });
                }
            });

            let init_elapsed = start.elapsed();
            tracing::info!(
                "BedCode Desktop initialized - WebSocket server on port {} (后端初始化耗时: {}ms)",
                ws_port,
                init_elapsed.as_millis()
            );

            // 注册核心模块的生命周期钩子（Shutdown/WindowClose）
            system::lifecycle::register_core_lifecycle_hooks();
            system::lifecycle::register_window_close_hooks();

            // 触发 Startup 钩子
            tauri::async_runtime::spawn(async move {
                system::lifecycle::lifecycle_registry().run_startup_hooks().await;
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
            // PTY Input
            commands::pty_input::write_to_session,
            commands::pty_input::send_special_key,
            // Pairing
            commands::system::generate_pairing_code,
            commands::system::get_current_pairing_code,
            commands::system::verify_pairing_code,
            commands::system::clear_pairing_code,
            commands::system::get_pairing_code_ttl,
            commands::system::set_pairing_code_ttl,
            commands::system::list_paired_devices,
            commands::system::remove_paired_device,
            commands::system::list_connection_history,
            commands::system::delete_connection_history,
            commands::system::set_log_level,
            commands::system::save_log_settings,
            commands::opener::open_log_dir,
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
            commands::system::set_terminal_bg_image,
            // Utility
            commands::system::ping,
            commands::system::get_app_version,
            commands::system::get_startup_time,
            commands::system::get_local_ip_addresses,
            commands::system::get_system_info,
            // 2026-08: append_terminal_output_dump 临时调试命令已注释禁用（恢复排查时取消注释）
            // commands::system::append_terminal_output_dump,
            commands::system::confirm_window_close,
            // Dev Console Relay（仅 dev：前端 console 日志转发，写 runtime.*.log + frontend.*.log 单独文件，见 commands::dev_logs）
            #[cfg(debug_assertions)]
            commands::dev_logs::report_frontend_log,
            commands::devices::get_connected_devices,
            // Plugin
            commands::plugin::plugin_list_loaded,
            commands::plugin::plugin_get_info,
            commands::plugin::plugin_preauthorize,
            commands::plugin::plugin_activate,
            commands::plugin::plugin_deactivate,
            commands::plugin::plugin_mark_error,
            commands::plugin::plugin_frontend_load_report,
            commands::plugin::plugin_get_activated_state,
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
            commands::plugin::plugin_fs_auth_respond,
            commands::opener::plugin_reveal_in_dir,
            // Server
            commands::server::server_start,
            commands::server::server_stop,
            commands::server::server_restart,
            commands::server::get_server_status,
            commands::server::get_local_ws_token,
            commands::server::get_server_metrics,
            commands::server::get_server_network_config,
            commands::server::update_server_port,
            commands::server::update_server_auto_start,
            commands::server::update_server_network_config,
            commands::server::get_traffic_encryption_config,
            commands::server::set_traffic_encryption_config,
            commands::server::get_link_crypto_fingerprint,
            commands::server::reset_server_network_config,
            // mDNS
            commands::mdns::mdns_start_advertise,
            commands::mdns::mdns_stop_advertise,
            commands::mdns::mdns_is_advertising,
            // Peer Net
            peer_net::start_peer_node,
            peer_net::stop_peer_node,
            // Phase 4（issue 13）：对等网络产品面已整体迁入 file-transfer 插件
            // （WIT host-peer 13 原语），主前端命令面退役——仅保留生命周期、
            // 首连确认与信任管理（宿主级兜底路径）。其余查询/管理命令的函数体
            // 暂留一版（部分仍为 host_impl 内部簿记调用），下版本删除。
            peer_net::respond_peer_consent,
            peer_net::list_trusted_peers,
            peer_net::revoke_trusted_peer,
            peer_receive::set_peer_receive_policy,
            peer_receive::set_peer_download_dir,
            peer_receive::set_peer_transfer_encryption,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // 使用 .build() + .run() 替代 .run()，以接入 Tauri RunEvent 循环
    // RunEvent::ExitRequested 是执行优雅关闭的最后时机
    app.run(move |_app_handle, event| match event {
        tauri::RunEvent::ExitRequested { .. } => {
            tracing::info!("BedCode Desktop exit requested, running shutdown hooks...");
            tauri::async_runtime::block_on(async {
                system::lifecycle::lifecycle_registry().run_shutdown_hooks().await;
            });
        }
        tauri::RunEvent::Exit { .. } => {
            tracing::info!("BedCode Desktop exited");
        }
        _ => {}
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
                // 尝试关闭主窗口（触发 CloseRequested → 生命周期钩子 → 确认弹窗）
                // 如果窗口已隐藏，先显示再关闭
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.close();
                } else {
                    // 无窗口时直接退出
                    app.exit(0);
                }
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
