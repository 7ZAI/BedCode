//! BedCode Mobile - Library Entry Point

pub mod auth;
pub mod commands;
pub mod connection;
pub mod egress;
pub mod enums;
pub mod file_service;
pub mod handler;
pub mod mdns;
pub mod model;
pub mod peer_migration;
pub mod peer_events;
pub mod peer_net;
pub mod peer_receive;
pub mod peer_remote;
pub mod peer_transfer;
pub mod plugin;
pub mod router;
pub mod state;
pub mod system;
pub mod terminal_stream_gateway;

/// 假插件端点夹具（票 01 基线与夹具）：本地 WS server 模拟桌面插件
/// `com.bedcode.terminal-session` 的 session-control / terminal 端点。
/// 单测（`#[cfg(test)]`）经此路径复用 `tests/support/mock_plugin_ws.rs`；
/// 集成测试在 `tests/` 内经 `#[path = "support/mock_plugin_ws.rs"]` 自行引入。
/// 仅测试构建生效（生产构建零影响）。
#[cfg(test)]
#[path = "../tests/support/mock_plugin_ws.rs"]
pub(crate) mod mock_plugin_ws;

// Re-export core types
pub use system::config;
pub use system::error::{AppError, Result};

use android_logger::Config;
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

            // 初始化 Egress Policy（授权记忆持久层加载）
            crate::egress::init(app_data_dir.clone());
            let settings_manager = Arc::new(SettingsManager::new(&app_data_dir)?);
            app.manage(settings_manager.clone());

            // 对等网络节点身份：复用上方 app_data_dir 解析点（决策 D3 宿主只注入
            // 目录），node_identity.json 与 auth 域 device_identity.json 并列存放；
            // 错误经 ? 上抛走既有启动失败路径——静默换身份会让对端可信列表全部失效
            crate::peer_net::init_node_identity(&app_data_dir)?;

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
            // 主库 schema 幂等建表（票 05：settings / plugin_storage / plugin_secrets /
            // plugin_auth_policies / plugin_auth_records——宿主机制数据 + 插件数据入库，
            // 与桌面 wasm-core db/schema.sql 同构；见 plugin/db_schema.rs）
            crate::plugin::db_schema::init_schema(&plugin_db.lock().expect("plugin db lock poisoned"))
                .map_err(|e| anyhow::anyhow!("Failed to init plugin DB schema: {}", e))?;

            // 票 05b：旧文件落盘 KV（plugins/*.json）→ 主库 plugin_storage 表一次性迁移；
            // 随后旧版对等网络数据迁移（写入 DB-backed 插件存储；幂等，失败均不阻断启动）
            {
                let storage = crate::plugin::storage::PluginStorage::new(plugin_db.clone());
                if let Err(e) = storage.migrate_file_store_to_db(&app_data_dir) {
                    tracing::warn!(error = %e, "legacy plugin storage file migration failed");
                }
                crate::peer_migration::migrate_legacy_peer_data(&storage, &app_data_dir);
            }

            // 创建插件管理器（WASM 运行时延迟初始化）
            let plugin_manager = crate::plugin::manager::PluginManager::new(
                &app_data_dir,
                settings_manager.clone(),
                plugin_db,
                Some(Arc::new(app_handle.clone())),
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
                    // - terminal-session: 远程终端控制端（终端订阅 + 任务域面板，
                    //   票 16 由 com.bedcode.auto-task 合并且换 id）
                    // - file-transfer: 内网文件传输插件，共享目录由用户在插件设置页
                    //   显式配置，信任模型 = 配对 + 用户显式目录白名单
                    for trusted_plugin in &["com.bedcode.terminal-session", "com.bedcode.file-transfer"] {
                        if let Err(e) = pm.fs_auth().add_plugin_whitelist(trusted_plugin).await {
                            tracing::warn!(plugin_id = %trusted_plugin, error = %e, "Failed to seed plugin whitelist");
                        }
                    }

                    pm.scan_and_load().await;
                    pm.load_all(&ah).await;
                });
            }

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
            // Egress Policy（外网授权弹窗回执 + 设置页查看/撤销 + 桌面端目标声明）
            commands::egress::egress_consent_resolve,
            commands::egress::egress_list_grants,
            commands::egress::egress_revoke_grants,
            commands::egress::egress_declare_desktop_target,
            // Egress 三档策略 + 记录管理（票 19：ADR 0022 2026-09-28 对齐）
            commands::egress::egress_get_strategy,
            commands::egress::egress_set_strategy,
            commands::egress::egress_list_records,
            commands::egress::egress_revoke_record,
            commands::egress::egress_purge_plugin,
            // HTTP 代理（ticket 03：统一请求出口，request_id 多路复用）
            commands::http_proxy::http_request,
            commands::http_proxy::http_cancel,
            // Connection Commands
            commands::connection::ws_connect,
            commands::connection::ws_disconnect,
            commands::connection::ws_get_status,
            commands::connection::ws_is_connected,
            commands::connection::ws_reconnect,
            commands::connection::set_auto_reconnect,
            commands::connection::get_ws_token,
            commands::connection::get_ws_url,
            // Terminal Link（票 12 协议客户端已迁插件 com.bedcode.terminal-session）：
            // terminal_subscribe / unsubscribe / unsubscribe_all / remove /
            // send_input / ack_rendered / get_history / get_state 八命令已随
            // terminal_link.rs 退役，前端走插件命令面（plugin_invoke）；
            // 段2 页面 Channel 登记是 Tauri 传输机制，保留在宿主窄转发层
            // （terminal_stream_gateway——ADR 0022 四类薄壳④）
            terminal_stream_gateway::terminal_page_subscribe,
            terminal_stream_gateway::terminal_page_unsubscribe,
            // Auth Commands（票 14 阶段 B：配对 / QR / 生物挑战编排已迁
            // com.bedcode.terminal-session 插件，宿主只余引擎事实面）
            commands::auth::ws_authenticate,
            commands::auth::ws_get_auth_credentials,
            commands::auth::ws_bind_biometric_credential,
            commands::auth::ws_unbind_biometric_credential,
            commands::auth::ws_get_biometric_key_status,
            // Session Commands（票 13：会话控制整体迁插件 com.bedcode.terminal-session——
            // 三命令零消费者随 session.rs / commands/session.rs 退役；前端走
            // src/plugin/sessionCommands.ts 的 plugin_invoke 命令面）
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
            // 对等网络传输 / 接收 / 远端浏览 —— 票 09 + 票 10 后**前端命令面为零**：
            // ① 票 09 摘掉发现投影与连接编排面（设备列表 / 缓存解析拨号 / 共享目录
            //    注册表 CRUD / 节点启停 / 首连应答 / 信任管理）；
            // ② 票 10 摘掉传输调度面（取消 / 暂停 / 恢复 / 选源 / 接收设置读面 /
            //    加密开关 / 并发上限 / 浏览 / 拉取）。
            // 真入口只有两条：① 插件 activate-deactivate 外壳（节点生命周期）
            // ② WIT host-peer 原语（拨号 / 发送 / 暂停恢复 / 应答 / 策略 / 落点 /
            //    共享根镜像 / 浏览 / 拉取）与 host-platform 选源原语。
            // 防回接锁：tests/retired_mobile_peer_transfer_command_face_lock.rs
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
            // System Open 配套：「所有文件访问」授权引导（system:open 权限）
            crate::plugin::commands::plugin_open_all_files_access,
            // System Open 配套：打开公共下载目录（设置页下载目录区「打开」，system:open 权限）
            crate::plugin::commands::plugin_open_download_dir,
            // File Service Commands（插件 TS 通道）
            // v2 批量传输批准（接收策略 / 异步批量批准）
            // SAF 存储访问（SafIo 主 seam，共享目录/上传页）
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    tracing::info!("BedCode application closed");
}
