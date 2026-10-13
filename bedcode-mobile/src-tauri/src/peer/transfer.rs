//! 发送面适配（委托共享 peer 能力域 crate）+ 平台原语 pick-files。
//!
//! 口径（2026-10-10 能力域集中化）：发送编排 / 会话句柄表 / 事件桥真源在
//! `packages/bedcode-server-peer-net`（`peer_engine_transfer`）；本文件只保留：
//!
//! - **委托**：host_ports / 插件生命周期用的发送面入口（签名逐字保留）；
//! - **移动端组合语义**：单一 `pause-transfer` / `resume-transfer` 原语同时管两个
//!   方向（WIT 移动 13 原语集）——发送批未命中时回落接收方向；
//! - **平台原语**：`peer_pick_files`（SAF 选择器 → `_data` 直读优先 / SAF 中转
//!   复制兜底）与 relay 复制——属 `host-platform` 平台面，不随引擎集中化迁移。

use tauri::{AppHandle, Manager};

pub use bedcode_server_peer_net::peer_engine_transfer::PeerTransferState;

use super::net::peer_ctx;

// ==================== 引擎入口委托 ====================

/// 取消进行中的发送会话（幂等：句柄不在册返回 false）；serve 供流批经引擎会话表取消
pub(crate) async fn cancel_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::cancel_transfer_for_plugin(peer_ctx(&app), batch_id).await
}

/// 显式暂停批（数据面门控）：发送批未命中 → 回落接收方向（push 接收批 / 拉取会话）
pub(crate) async fn pause_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let ctx = peer_ctx(&app);
    if bedcode_server_peer_net::pause_transfer_for_plugin(ctx.clone(), batch_id.clone()).await? {
        return Ok(true);
    }
    bedcode_server_peer_net::peer_engine_receive::pause_peer_receiving(ctx, batch_id).await
}

/// 恢复暂停批：发送批未命中 → 回落接收方向（写 Resume 帧续流）
pub(crate) async fn resume_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let ctx = peer_ctx(&app);
    if bedcode_server_peer_net::resume_transfer_for_plugin(ctx.clone(), batch_id.clone()).await? {
        return Ok(true);
    }
    bedcode_server_peer_net::peer_engine_receive::resume_peer_receiving(ctx, batch_id).await
}

/// 发送入口：一次调用 = 一个会话立即发起（目录递归收集源清单后直接拨号启动）；
/// 并发节流由调用方（插件）在调用前自控。返回传输句柄（batch-id）。
pub(crate) async fn send_files_to_peer_with_policy(
    app: AppHandle,
    node_id: String,
    paths: Vec<String>,
    encrypt_override: Option<bool>,
) -> crate::Result<String> {
    bedcode_server_peer_net::send_files_for_plugin(peer_ctx(&app), node_id, paths, encrypt_override).await
}

/// 发送源收集（host-peer `collect-outgoing` 原语入口；纯元数据面）
pub(crate) async fn collect_outgoing_for_plugin(paths: Vec<String>) -> crate::Result<String> {
    bedcode_server_peer_net::collect_outgoing_for_plugin(paths).await
}

/// 活跃传输批清单（host-peer `active-transfers` 入口）：send 句柄表 + receive
/// pending 询问表 + pull 会话表三处引擎控制面的聚合投影（JSON）
pub(crate) async fn active_transfers_json(app: &AppHandle) -> crate::Result<String> {
    bedcode_server_peer_net::active_transfers_for_plugin(peer_ctx(app)).await
}

// ==================== 发送源选择（平台原语，保留） ====================
//
// 选择待发送文件（Android：SAF 选择器 → 可读路径优先 / SAF 中转复制兜底；
// 单次单个，可多次累加）。属 `host-platform` 平台原语（与传输编排无关），
// 不随引擎集中化迁移。

/// 分区存储下 `_data` 直读路径（/storage/emulated/0/...）经 std::fs::open 会
/// EACCES（真机实证：上传 72MB 文件 `Permission denied (os error 13)`）——
/// 需先探活可读性，不可读则把 content URI 中转复制到 app 私有 cache 再发送
/// （私有路径 std::fs 恒可读）。
///
/// 真入口 = host-platform `pick-files`（票 10 起无前端命令面）
pub(crate) async fn peer_pick_files(app_handle: AppHandle) -> crate::Result<Vec<String>> {
    let Some(meta) = crate::plugin::android_plugins::pick_file_android_meta().await? else {
        // 用户取消选择
        return Ok(Vec::new());
    };
    // 快速路径：`_data` 直读路径可打开（All Files Access 已授权等场景）→ 零复制
    if !meta.data_path.is_empty() && std::fs::File::open(&meta.data_path).is_ok() {
        return Ok(vec![meta.data_path]);
    }
    // 分区存储兜底：content URI → app 私有 cache 中转复制（保留原始文件名）
    if meta.uri.is_empty() {
        return Err(crate::AppError::Plugin(
            "file pick returned neither readable path nor SAF uri".to_string(),
        ));
    }
    let dest_name = relay_upload_dest_name(&meta.display_name);
    let path = relay_copy_upload_source(&app_handle, &meta.uri, &dest_name).await?;
    Ok(vec![path])
}

async fn relay_copy_upload_source(app: &AppHandle, uri: &str, dest_name: &str) -> crate::Result<String> {
    let saf = app.state::<crate::plugin::saf_io::SafIoState>();
    let handle = saf
        .read_to_cache(uri, dest_name)
        .map_err(|e| crate::AppError::Plugin(format!("upload relay copy start failed: {e}")))?;
    loop {
        let status = saf
            .copy_status(&handle.copy_id)
            .map_err(|e| crate::AppError::Plugin(format!("upload relay copy poll failed: {e}")))?;
        if status.finished {
            if status.cancelled {
                return Err(crate::AppError::Plugin("upload relay copy cancelled".to_string()));
            }
            if let Some(err) = status.error {
                return Err(crate::AppError::Plugin(format!("upload relay copy failed: {err}")));
            }
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    Ok(handle.dest_path)
}

/// 上传源中转副本文件名：保留原始名；空/含路径分隔符兜底为安全名
fn relay_upload_dest_name(display_name: &str) -> String {
    let name: String = display_name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    let name = name.trim().to_string();
    if name.is_empty() || name == "." || name == ".." {
        "upload-file".to_string()
    } else {
        name
    }
}
