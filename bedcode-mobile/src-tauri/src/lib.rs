//! BedCode Mobile - Library Entry Point

pub mod auth;
pub mod commands;
pub mod connection;
pub mod enums;
pub mod file_service;
pub mod handler;
pub mod mdns;
pub mod model;
pub mod peer_migration;
pub mod peer_net;
pub mod peer_receive;
pub mod peer_remote;
pub mod peer_transfer;
pub mod plugin;
pub mod router;
pub mod session;
pub mod state;
pub mod system;

// Re-export core types
pub use system::config;
pub use system::error::{AppError, Result};

use android_logger::Config;
use connection::PairingService;
use log::LevelFilter;
use std::sync::Arc;
use tauri::Manager;

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use crate::system::settings::SettingsManager;

    // 尽可能早地初始化日志
    // tracing 的 "log" feature 将 tracing:: 宏自动转发到 log crate
    // android_logger 将 log:: 输出发送到 adb logcat
    //
    // 级别：dev 构建打满 Debug（开发期 logcat 全量）；release 收敛到 Info——
    // Android logcat 主缓冲是系统级环形（每 app 默认约 256KB~1MB），release 打
    // Debug 会占满缓冲导致关键日志被系统丢弃，且泄露内部路径等调试信息
    #[cfg(debug_assertions)]
    let log_level = LevelFilter::Debug;
    #[cfg(not(debug_assertions))]
    let log_level = LevelFilter::Info;
    android_logger::init_once(Config::default().with_max_level(log_level).with_tag("BedCode"));
    tracing::info!("BedCode Mobile early logging init (tracing → log → logcat, level={log_level})");

    tracing::info!("Building Tauri application...");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_edge_to_edge::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_machine_uid::init())
        .plugin(crate::plugin::android_plugins::asset_extractor_plugin())
        .plugin(crate::plugin::android_plugins::foreground_service_plugin())
        .plugin(crate::plugin::android_plugins::task_notification_plugin())
        .plugin(crate::plugin::android_plugins::biometric_key_plugin())
        .plugin(crate::plugin::android_plugins::downloads_dir_plugin())
        .plugin(crate::plugin::android_plugins::file_delete_plugin())
        .plugin(crate::plugin::android_plugins::device_info_plugin())
        .plugin(crate::plugin::android_plugins::saf_picker_plugin())
        .plugin(crate::plugin::android_plugins::saf_transfer_plugin())
        .plugin(crate::plugin::android_plugins::all_files_access_plugin())
        .plugin(crate::plugin::android_plugins::multicast_lock_plugin())
        .plugin(crate::plugin::android_plugins::status_bar_style_plugin())
        .setup(|app| {
            tracing::info!("BedCode setup starting...");
            tracing::info!("Plugins initialized");

            let app_handle = app.handle();

            // 窗口焦点监听（后台/锁屏判定：批量传输请求系统通知用）

            // 托管 SafIo 主 seam 实现（Android = KotlinSafIo 转发 SafTransferPlugin；
            // 其他平台 = 明确不可用）。经 state 注入命令层，测试可替换为 fake
            app.manage(crate::plugin::saf_io::SafIoState(
                crate::plugin::saf_io::default_saf_io(),
            ));

            // 初始化移动端设置管理器 (JSON 文件存储)
            let app_data_dir = app_handle.path().app_data_dir().expect("Failed to get app data dir");
            let settings_manager = Arc::new(SettingsManager::new(&app_data_dir)?);
            app.manage(settings_manager.clone());

            // 对等网络节点身份：复用上方 app_data_dir 解析点（决策 D3 宿主只注入
            // 目录），node_identity.json 与 auth 域 device_identity.json 并列存放；
            // 错误经 ? 上抛走既有启动失败路径——静默换身份会让对端可信列表全部失效
            crate::peer_net::init_node_identity(&app_data_dir)?;

            // 旧版对等网络数据一次性迁移（issue 13 Phase 4 步骤 9；幂等，失败不阻断）
            crate::peer_migration::migrate_legacy_peer_data(&app_data_dir);

            // 对等网络节点状态容器 + 自动启动（ticket 03，决策 D7）：异步装配节点
            // 与 mDNS 发现守护；启动前经 Kotlin MulticastLockPlugin 申请多播锁
            // （Android 收包前提，D6）
            app.manage(crate::peer_net::PeerNetState::default());
            app.manage(crate::peer_transfer::PeerTransferState::default());
            app.manage(crate::peer_receive::PeerReceiveState::default());
            app.manage(crate::peer_remote::PeerRemoteState::default());
            // 节点自启已退役：peer-net 生命周期随文件传输插件启停
            // （插件管理器 activate/deactivate 外壳接线，见 peer_net::ensure_node_started）

            // 创建插件数据库连接（WASM Host Function 使用；
            // std Mutex：SQL 为同步操作，host fn 同步取锁，避免 block_on 绕行）
            let db_path = app_data_dir.join("bedcode_plugins.db");
            let plugin_db = Arc::new(std::sync::Mutex::new(
                rusqlite::Connection::open(&db_path).map_err(|e| anyhow::anyhow!("Failed to open plugin DB: {}", e))?,
            ));

            // 创建插件管理器（WASM 运行时延迟初始化）
            let plugin_manager = crate::plugin::manager::PluginManager::new(
                &app_data_dir,
                settings_manager.clone(),
                plugin_db,
                Some(Arc::new(app_handle.clone())),
            );
            let plugin_manager = crate::state::init_plugin_manager(Arc::new(plugin_manager));
            app.manage(plugin_manager.clone());

            // 监听前端 terminal_output_activity（前端直连终端 WS 收到输出帧时触发
            // 插件 TerminalOutput 通知；输出不再经 Rust 中转，见 ticket 09）
            // 必须在 init_plugin_manager 之后注册：内部会取全局插件管理器，
            // 早于初始化调用会触发 OnceLock panic（PluginManager not initialized）
            crate::router::event::init_terminal_output_listener(app_handle);

            // 异步：解压内置插件 → 初始化 WASM 运行时 → 扫描加载 → 自动激活
            // 使用 tauri::async_runtime::spawn 而非 tokio::spawn，
            // 因为 setup 闭包不在 Tokio 运行时上下文中执行，tokio::spawn 会 panic
            {
                let pm = plugin_manager;
                let ah = app_handle.clone();
                let app_version = app.package_info().version.to_string();
                let app_data_dir_for_extract = app_data_dir.clone();
                tauri::async_runtime::spawn(async move {
                    // 采集并挂载全局系统信息（OS / 设备名称 / IP），
                    // 并同步设备名到 AuthManager，配对时上报真实用户设备名
                    let system_info = crate::system::info::SystemInfo::collect().await;
                    let device_name = system_info.device_name.clone();
                    crate::state::init_system_info(system_info);
                    crate::state::get_auth_manager()
                        .set_device_name(device_name.clone())
                        .await;
                    tracing::info!(
                        "[BedCode] System info initialized: device_name={}, os={}",
                        device_name,
                        std::env::consts::OS
                    );

                    // 解压内置插件（Android：Kotlin 桥；桌面 dev：源码资源目录复制）
                    if let Err(e) = crate::plugin::loader::PluginLoader::extract_apk_plugins(
                        &app_data_dir_for_extract,
                        &app_version,
                    )
                    .await
                    {
                        tracing::warn!("Failed to extract bundled plugins: {}", e);
                    }

                    // 在 Tokio 运行时上下文中初始化 WASM 运行时
                    if let Err(e) = pm.init_wasm_runtime().await {
                        tracing::error!("Failed to init WASM runtime: {}", e);
                        return;
                    }

                    // 种子内置受信任插件白名单（幂等：已存在则跳过）
                    // - auto-task: 自动化任务插件
                    // - file-transfer: 内网文件传输插件，共享目录由用户在插件设置页
                    //   显式配置，信任模型 = 配对 + 用户显式目录白名单
                    for trusted_plugin in &["com.bedcode.auto-task", "com.bedcode.file-transfer"] {
                        if let Err(e) = pm.fs_auth().add_plugin_whitelist(trusted_plugin).await {
                            tracing::warn!(plugin_id = %trusted_plugin, error = %e, "Failed to seed plugin whitelist");
                        }
                    }

                    pm.scan_and_load().await;
                    pm.load_all(&ah).await;
                });
            }

            let pairing_service = Arc::new(PairingService::new());
            app.manage(pairing_service);

            // 初始化 mDNS 管理器（内部字段级锁，实例不可变，无需外层 RwLock）
            let mdns_discovery = Arc::new(crate::mdns::discovery::MdnsDiscovery::new());
            app.manage(mdns_discovery);
            let mdns_advertiser = Arc::new(crate::mdns::advertiser::MdnsAdvertiser::new());
            app.manage(mdns_advertiser);

            tracing::info!("BedCode Mobile started successfully!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Token Commands (merged into connection)
            commands::connection::ws_set_token,
            commands::connection::ws_get_token,
            commands::connection::ws_clear_token,
            // Link Crypto Context（issue 09：事件 WS 链路加密桥）
            commands::connection::set_link_crypto_context,
            // Connection Commands
            commands::connection::ws_connect,
            commands::connection::ws_disconnect,
            commands::connection::ws_get_status,
            commands::connection::ws_is_connected,
            commands::connection::ws_reconnect,
            commands::connection::get_ws_token,
            commands::connection::get_ws_url,
            // Auth Commands
            commands::auth::ws_get_auth_status,
            commands::auth::ws_authenticate,
            commands::auth::ws_request_pairing,
            commands::auth::ws_verify_pairing_code,
            commands::auth::ws_authenticate_with_qr,
            commands::auth::ws_authenticate_with_biometric,
            commands::auth::ws_bind_biometric_credential,
            commands::auth::ws_unbind_biometric_credential,
            commands::auth::ws_get_biometric_key_status,
            // Session Commands
            commands::session::ws_load_sessions,
            commands::session::ws_join_session,
            commands::session::get_terminal_ws_info,
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
            system::commands::get_system_info,
            system::commands::get_local_ip_addresses,
            // Android Specific
            commands::android::set_screen_orientation,
            commands::android::keep_screen_awake,
            commands::android::open_url_in_browser,
            commands::android::set_status_bar_style,
            // Session Config (移动端使用内存存储)
            commands::mobile_commands::list_session_configs_mobile,
            commands::mobile_commands::get_session_config_mobile,
            // mDNS
            commands::mdns::mdns_start_discovery,
            commands::mdns::mdns_stop_discovery,
            commands::mdns::mdns_get_discovered_services,
            commands::mdns::mdns_start_advertise,
            commands::mdns::mdns_stop_advertise,
            // Peer Net
            peer_net::start_peer_node,
            peer_net::stop_peer_node,
            peer_net::list_discovered_peers,
            peer_net::dial_peer,
            peer_net::disconnect_peer,
            peer_net::respond_peer_consent,
            peer_net::list_trusted_peers,
            peer_net::revoke_trusted_peer,
            peer_net::list_shared_directories,
            peer_net::add_shared_directory_saf,
            peer_net::remove_shared_directory,
            // Peer Transfer (issue 09 发送侧)
            peer_transfer::send_files_to_peer,
            peer_transfer::cancel_peer_transfer,
            peer_transfer::retry_peer_transfer,
            peer_transfer::list_peer_transfers,
            peer_transfer::clear_peer_transfer_history,
            peer_transfer::peer_pick_files,
            peer_transfer::peer_pick_folder,
            // Peer Receive (issue 10 接收侧)
            peer_receive::list_peer_receiving,
            peer_receive::respond_peer_transfer,
            peer_receive::cancel_peer_receiving,
            peer_receive::get_peer_receive_settings,
            peer_receive::set_peer_receive_policy,
            peer_receive::set_peer_transfer_encryption,
            peer_receive::clear_peer_receiving_history,
            // Peer Remote (issue 11 远端浏览/拉取)
            peer_remote::list_peer_shared_roots,
            peer_remote::browse_peer_directory,
            peer_remote::pull_peer_files,
            // Plugin Commands
            crate::plugin::commands::plugin_list_loaded,
            crate::plugin::commands::plugin_get_info,
            crate::plugin::commands::plugin_preauthorize,
            crate::plugin::commands::plugin_activate,
            crate::plugin::commands::plugin_deactivate,
            crate::plugin::commands::plugin_is_enabled,
            crate::plugin::commands::plugin_set_enabled,
            crate::plugin::commands::plugin_mark_error,
            crate::plugin::commands::plugin_report_ready,
            crate::plugin::commands::plugin_storage_get,
            crate::plugin::commands::plugin_storage_set,
            crate::plugin::commands::plugin_storage_delete,
            crate::plugin::commands::plugin_download,
            crate::plugin::commands::plugin_install_from_file,
            crate::plugin::commands::plugin_uninstall,
            crate::plugin::commands::reload_wasm_plugin,
            // File System Auth Commands
            crate::plugin::commands::plugin_fs_auth_respond,
            crate::plugin::commands::plugin_fs_add_path_whitelist,
            crate::plugin::commands::plugin_fs_remove_path_whitelist,
            crate::plugin::commands::plugin_fs_get_path_whitelist,
            crate::plugin::commands::plugin_fs_add_plugin_whitelist,
            crate::plugin::commands::plugin_fs_remove_plugin_whitelist,
            crate::plugin::commands::plugin_fs_get_plugin_whitelist,
            crate::plugin::commands::plugin_log,
            crate::plugin::commands::plugin_invoke,
            // Dev Console Relay（仅 debug 构建：前端 console 日志转发 → logcat → dev:log 电脑端落盘，见 commands::dev_logs）
            #[cfg(debug_assertions)]
            commands::dev_logs::report_frontend_log,
            // System Open（历史「打开所在文件夹」真机路径，system:open 权限）
            crate::plugin::commands::plugin_reveal_received_file,
            // File Service Commands（插件 TS 通道）
            // v2 批量传输批准（接收策略 / 异步批量批准）
            // SAF 存储访问（SafIo 主 seam，共享目录/上传页）
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    tracing::info!("BedCode application closed");
}
