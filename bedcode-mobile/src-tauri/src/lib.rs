//! BedCode Mobile - Library Entry Point

pub mod auth;
pub mod commands;
pub mod connection;
pub mod enums;
pub mod file_service;
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
        .setup(|app| {
            tracing::info!("BedCode setup starting...");
            tracing::info!("Plugins initialized");

            let app_handle = app.handle();

            // 窗口焦点监听（后台/锁屏判定：批量传输请求系统通知用）
            crate::file_service::notify::attach_focus_listener(app_handle);

            // 托管 SafIo 主 seam 实现（Android = KotlinSafIo 转发 SafTransferPlugin；
            // 其他平台 = 明确不可用）。经 state 注入命令层，测试可替换为 fake
            app.manage(crate::plugin::saf_io::SafIoState(
                crate::plugin::saf_io::default_saf_io(),
            ));

            // 初始化移动端设置管理器 (JSON 文件存储)
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            let settings_manager = Arc::new(SettingsManager::new(&app_data_dir)?);
            app.manage(settings_manager.clone());

            // 创建插件数据库连接（WASM Host Function 使用；
            // std Mutex：SQL 为同步操作，host fn 同步取锁，避免 block_on 绕行）
            let db_path = app_data_dir.join("bedcode_plugins.db");
            let plugin_db = Arc::new(std::sync::Mutex::new(
                rusqlite::Connection::open(&db_path)
                    .map_err(|e| anyhow::anyhow!("Failed to open plugin DB: {}", e))?
            ));

            // 创建插件管理器（WASM 运行时延迟初始化）
            let plugin_manager = crate::plugin::manager::PluginManager::new(
                &app_data_dir,
                settings_manager.clone(),
                plugin_db,
                Arc::new(app_handle.clone()),
            );
            let plugin_manager = crate::state::init_plugin_manager(Arc::new(plugin_manager));
            app.manage(plugin_manager.clone());

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
                    let system_info =
                        crate::system::info::SystemInfo::collect().await;
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
                    ).await {
                        tracing::warn!("Failed to extract bundled plugins: {}", e);
                    }

                    // 在 Tokio 运行时上下文中初始化 WASM 运行时
                    if let Err(e) = pm.init_wasm_runtime().await {
                        tracing::error!("Failed to init WASM runtime: {}", e);
                        return;
                    }

                    // 注入 AppHandle 到文件服务注册表：双通道推送（Tauri 事件 + 插件
                    // 总线）的 Tauri 事件通道依赖它。WASM 插件经 host_filesrv_mount
                    // 挂载时不注入（仅 TS 通道 plugin_filesrv_mount 注入），若此处
                    // 缺失，filesrv:peer_changed 事件到不了插件前端，对端永远显示
                    // "未共享"。必须在插件激活（scan_and_load）前注入一次（幂等）
                    crate::state::get_file_service()
                        .registry
                        .set_app_handle(ah.clone())
                        .await;

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
            commands::auth::ws_authenticate_with_biometric,
            commands::auth::ws_bind_biometric_credential,
            commands::auth::ws_unbind_biometric_credential,
            commands::auth::ws_get_biometric_key_status,
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
            system::commands::get_system_info,
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
            // File Service Commands（插件 TS 通道）
            crate::plugin::commands::plugin_filesrv_mount,
            crate::plugin::commands::plugin_filesrv_update_roots,
            crate::plugin::commands::plugin_filesrv_dispose,
            crate::plugin::commands::plugin_filesrv_respond_upload_request,
            crate::plugin::commands::plugin_filesrv_get_peer,
            // v2 批量传输批准（接收策略 / 异步批量批准）
            crate::plugin::commands::plugin_filesrv_approve_transfer,
            crate::plugin::commands::plugin_filesrv_reject_transfer,
            crate::plugin::commands::plugin_filesrv_set_approval_timeout,
            crate::plugin::commands::plugin_filesrv_cancel_receiving,
            crate::plugin::commands::plugin_filesrv_respond_transfer_request,
            crate::plugin::commands::plugin_open_file,
            crate::plugin::commands::plugin_open_file_location,
            crate::plugin::commands::plugin_pick_directory,
            crate::plugin::commands::plugin_pick_file,
            crate::plugin::commands::open_all_files_settings,
            // SAF 存储访问（SafIo 主 seam，共享目录/上传页）
            crate::plugin::commands::plugin_saf_list_tree,
            crate::plugin::commands::plugin_saf_copy_start,
            crate::plugin::commands::plugin_saf_copy_status,
            crate::plugin::commands::plugin_saf_copy_cancel,
            crate::plugin::commands::plugin_saf_cleanup_stale_copies,
            crate::plugin::commands::plugin_saf_check_authorized,
            crate::plugin::commands::plugin_pick_shared_directory,
            crate::plugin::commands::plugin_saf_list_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    tracing::info!("BedCode application closed");
}
