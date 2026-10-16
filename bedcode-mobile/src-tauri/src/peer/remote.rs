//! 远端浏览 / 拉取适配（委托共享 peer 能力域 crate）。
//!
//! 口径（2026-10-10 能力域集中化，能力域集中化 spec 票 04 · R2/R3）：
//! 浏览 / 拉取 / 会话令牌表 / 建行事实事件真源在
//! `packages/bedcode-server-peer-net`（`peer_engine_remote`）；本文件只保留
//! 委托入口（签名与 DTO 逐字保留——DTO 由能力域再导出，wire 形状零变化）。
//!
//! **按拍板退役的移动端遗留（R3）**：`PULL_FILES_CAP`（512）与并发信号量
//! （读已无人写的 `concurrency` 设置）不迁——能力域既定口径是「逐文件会话立即
//! 发起，并发节流归调用方（插件）分批自控」（见 crate `peer_engine_remote` 头注）。

use tauri::AppHandle;

pub use bedcode_server_peer_net::peer_engine_remote::{
    PeerRemoteState, PeerSharedRootDto, RemoteBrowseDto, RemoteEntryDto, RemotePullFileDto,
};

use super::net::peer_ctx;

/// 对端共享目录根列表（host-peer `list-remote-roots` 入口）
pub(crate) async fn list_peer_shared_roots(app: AppHandle, node_id: String) -> crate::Result<Vec<PeerSharedRootDto>> {
    bedcode_server_peer_net::list_remote_roots_for_plugin(peer_ctx(&app), node_id).await
}

/// 远端目录浏览（单请求会话；host-peer `browse-directory` 入口）
pub(crate) async fn browse_peer_directory(
    app: AppHandle,
    node_id: String,
    dir_id: String,
    rel_path: String,
) -> crate::Result<RemoteBrowseDto> {
    bedcode_server_peer_net::browse_remote_for_plugin(peer_ctx(&app), node_id, dir_id, rel_path).await
}

/// 拉取文件（host-peer `pull-files` 入口）：逐文件会话立即发起，返回本次发起
/// 会话数（每批 batch_id 由引擎铸造并经 `pull-started` 事件回灌建行）
pub(crate) async fn pull_peer_files(
    app: AppHandle,
    node_id: String,
    dir_id: String,
    files: Vec<RemotePullFileDto>,
) -> crate::Result<u32> {
    bedcode_server_peer_net::pull_files_for_plugin(peer_ctx(&app), node_id, dir_id, files).await
}
