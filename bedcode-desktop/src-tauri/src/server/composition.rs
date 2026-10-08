//! 宿主壳组合根（server-lib-split 票 03 / 04 / 05 收口）
//!
//! `bedcode-server-core::app::serve` 只认 [`TransportFace`]，内核不再反向认识传输面。
//! 本文件是宿主侧**唯一**同时看见两个传输面的装配点（原 `server/core/app.rs` 的 I3
//! 组合物豁免点的归宿）：票 04/05 后两个面的 face 实现都已随面下沉进各自 crate，
//! 这里只剩「把 faces 交给 `serve`」的纯壳——谁都不认识谁，横向依赖在清单层面就不成立。
//!
//! 另含链路加密的启动薄壳：core 的 `link_crypto` 不认识 `tauri::AppHandle` 与宿主
//! `Database`，数据目录与主库连接在此解析后传入。
//!
//! **端口注册表的唯一装配点**（server-lib-split 票 07）：`install_server_ports` 是
//! 进程内**唯一**调 `bedcode_server_base::ports::init` 的函数——GUI bootstrap
//! （`lib.rs` setup）与无头 harness（`cross-end-tests` 的 `desktop_ctx`）都走它。
//! 曾出现过「只有 GUI 装配、无头 rig 不装配」的分裂，症状是网关
//! `ports::get() == None` 判 `PassThrough`，插件注册的全部宿主别名一律 404
//! （见 `crates_carry_their_own_test_contracts_are_not_silently_skipped` 与
//! `cross-end-tests` 的 `rig_assembles_the_same_server_ports_face_as_gui_boot`）。

use bedcode_server_base::config::NetworkConfig;
use bedcode_server_core::TransportFace;
use std::sync::Arc;

use crate::db::Database;

// ==================== 端口注册表装配 ====================

/// 装配全局 server 端口（宿主壳实现注入进 `bedcode_server_base::ports`）
///
/// 无头 / 单测装配面（无 `AppHandle`）：路径面回退 `AppContext::try_global()`，
/// 总线面 late-bound（见 [`crate::server::ports_impl::assemble`]）。
///
/// 幂等语义沿用 `ports::init`：重复装配 panic（装配点唯一，重复即装配面分裂）。
pub fn install_server_ports() {
    install_server_ports_with(None);
}

/// 装配全局 server 端口并带上宿主句柄（GUI bootstrap 调用）
///
/// 句柄直取给路径面：端口**可能早于 `AppContext` 注册**被装配——插件激活发生在
/// `PluginHost::new` 内部（激活期 guest 立刻调 `host-*` 原语），而组合根在其返回后
/// 才注册 `AppContext`。只靠全局取句柄会让这段窗口内的路径解析恒定失败
/// （2026-10-07 实机：file-transfer 的 `host-peer.start-node` 落在窗口内，
/// peer 节点起不来 → 桌面端不广播 → 移动端发现不到桌面）。
///
/// 仍然保留 late-bound 的总线面：真实总线在 `PluginHost` 构造时创建，早于端口装配
/// 但晚于部分端口使用方，钉死会指向占位总线。
pub fn install_server_ports_with(app_handle: Option<Arc<tauri::AppHandle>>) {
    bedcode_server_base::ports::init(crate::server::ports_impl::assemble(app_handle));
}

// ==================== 传输面装配 ====================

/// 装配 faces（顺序 = actix 中间件由外向内；http 的 `TrafficFilter` 落在最内层）
pub fn transport_faces() -> Vec<Arc<dyn TransportFace>> {
    vec![
        Arc::new(bedcode_server_http::HttpTransportFace),
        Arc::new(bedcode_server_websocket::WebSocketTransportFace),
    ]
}

/// 启动 Actix Web 服务器（HTTP + WebSocket 统一端口）
///
/// 兼容面：签名与拆分前的 `server::core::app::start_http_server` 一致，宿主调用点
/// 与集成测试二进制不必改动；实际装配委托 [`bedcode_server_core::app::serve`]。
///
/// 返回 `ServerHandle` 用于优雅停机
/// 调用方通过 oneshot channel 获取 handle，然后继续 await server 保持运行
pub async fn start_http_server(
    port: u16,
    config: &NetworkConfig,
) -> std::io::Result<(
    actix_web::dev::ServerHandle,
    impl std::future::Future<Output = std::io::Result<()>>,
)> {
    bedcode_server_core::app::serve(port, config, transport_faces()).await
}

// ==================== 链路加密启动装配 ====================

/// 启动期链路加密装配薄壳：解析应用数据目录与主库连接后交 core（spec §6：
/// 必须在 `supervisor.start` 之前完成，第一条流量就要被开关裁决）
///
/// 数据目录解析失败时不走 core 的正常装配：core 的 `init_at_startup` 需要目录
/// 建身份，而拆分前这条分支的行为是「error 日志 + 读 DB 配置但强制全关」，
/// 身份不可用时静默换钥比全关更糟，故该 fail-safe 在此逐字保持。
pub async fn init_link_crypto_at_startup(app_handle: &tauri::AppHandle) {
    use tauri::Manager;

    let dir = app_handle.path().app_data_dir();
    let db = app_handle.state::<Arc<tokio::sync::Mutex<Database>>>();
    let guard = db.lock().await;

    match dir {
        Ok(dir) => bedcode_server_core::link_crypto::init_at_startup(&dir, guard.conn()),
        Err(e) => {
            tracing::error!("app data dir unavailable ({e}), link crypto stays off");
            let mut config = bedcode_server_core::link_crypto::load_config_from_db(guard.conn());
            config.enabled = false;
            bedcode_server_core::link_crypto::update_config(config);
            bedcode_server_core::link_crypto::sync_registration();
        }
    }
}

/// 命令层链路身份指纹薄壳：core 只接数据目录，`AppHandle` → 目录解析留在宿主
pub fn link_crypto_identity_fingerprint(app_handle: &tauri::AppHandle) -> crate::Result<String> {
    use tauri::Manager;
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Internal(format!("app data dir unavailable for link identity: {e}")))?;
    bedcode_server_core::link_crypto::ensure_identity_fingerprint(&dir)
}
