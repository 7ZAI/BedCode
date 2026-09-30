//! peer-net Tauri 命令壳 + 上下文装配（宿主壳；server-lib-split）
//!
//! peer-net 拆 lib 后不再持有 `AppHandle`（引擎函数一律 `&PeerCtx` / `Arc<PeerCtx>`）：
//! 本模块是唯一还认识 `AppHandle` 的薄壳——[`peer_ctx`] 从 `AppHandle` 装配
//! [`PeerCtx`]（端口 + 四个引擎状态），五条前端命令在此保持 `#[tauri::command]`
//! 签名并把调用转给引擎实现。拆分前命令壳与引擎同文件，本模块是拆分产物；
//! 库函数本身不再标注 `#[tauri::command]`。

use bedcode_server_peer_net::{
    peer_engine_receive::PeerReceiveState, peer_engine_remote::PeerRemoteState,
    peer_engine_transfer::PeerTransferState, PeerCtx, PeerNetState, PeerNodeStatus, TrustedPeerDto,
};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// 从 AppHandle 装配 peer-net 操作上下文
///
/// 生产运行期四个状态均已 `app.manage(Arc::new(..))`、端口已装配；
/// `app.state::<Arc<T>>()` 在未 manage 时 panic（与既有 `app.state::<T>()`
/// 行为一致——命令面只存在于真实运行时）。端口缺失（理论不可达）时以
/// `ports_impl::assemble()` 占位兜底，避免装配面 panic。
pub fn peer_ctx(app: &AppHandle) -> Arc<PeerCtx> {
    Arc::new(PeerCtx {
        ports: bedcode_server_base::ports::get()
            .cloned()
            .unwrap_or_else(|| Arc::new(crate::server::ports_impl::assemble())),
        state: app.state::<Arc<PeerNetState>>().inner().clone(),
        transfer: app.state::<Arc<PeerTransferState>>().inner().clone(),
        receive: app.state::<Arc<PeerReceiveState>>().inner().clone(),
        remote: app.state::<Arc<PeerRemoteState>>().inner().clone(),
    })
}

// ==================== 前端命令面（薄壳，引擎实现见 peer_net） ====================

/// 启动对等网络节点（幂等：已启动直接返回现状）
#[tauri::command]
pub async fn start_peer_node(app: AppHandle) -> crate::Result<PeerNodeStatus> {
    bedcode_server_peer_net::start_peer_node(peer_ctx(&app)).await
}

/// 优雅关停对等网络节点（幂等：未启动直接成功）
#[tauri::command]
pub async fn stop_peer_node(app: AppHandle) -> crate::Result<()> {
    bedcode_server_peer_net::stop_peer_node(peer_ctx(&app)).await
}

/// 应答首连确认弹窗（issue 04 命令面）
#[tauri::command]
pub async fn respond_peer_consent(app: AppHandle, request_id: String, accepted: bool) -> crate::Result<bool> {
    bedcode_server_peer_net::respond_peer_consent(peer_ctx(&app), request_id, accepted).await
}

/// 可信对端列表（设置面管理用；节点未启动仍可读——句柄独立于运行时存活）
#[tauri::command]
pub async fn list_trusted_peers(app: AppHandle) -> crate::Result<Vec<TrustedPeerDto>> {
    bedcode_server_peer_net::list_trusted_peers(peer_ctx(&app)).await
}

/// 撤销可信对端（返回该 ID 原本是否存在；撤销后对端重连重新走首连确认）
#[tauri::command]
pub async fn revoke_trusted_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::revoke_trusted_peer(peer_ctx(&app), node_id).await
}
