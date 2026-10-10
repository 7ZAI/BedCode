//! 接收面适配（委托共享 peer 能力域 crate）。
//!
//! 口径（2026-10-10 能力域集中化，能力域集中化 spec 票 04 · R1/R3）：
//! 询问回执表 / 策略闸门 / 接收事件桥 / 活跃批投影真源在
//! `packages/bedcode-server-peer-net`（`peer_engine_receive`）；本文件只保留
//! 委托入口（签名逐字保留）。
//!
//! **两处按拍板退役的移动端遗留**：
//! - 接收设置落盘（`transfer_settings.json`，R1）：业务真源归插件（插件经
//!   `set-receive-policy` / `set-download-dir` 原语推送），宿主不再持有第二份真源；
//! - 接收任务表（票 07 已下沉）与 `node-stopped` 直推（R4 已收进能力域）。

use tauri::AppHandle;

pub use bedcode_server_peer_net::peer_engine_receive::PeerReceiveState;

use super::peer_net::peer_ctx;

/// 接收策略模式（wire/持久化字符串；前端分段控件直用）
pub const POLICY_ASK: &str = "ask";
pub const POLICY_ALWAYS_ACCEPT: &str = "always_accept";
pub const POLICY_ALWAYS_DENY: &str = "always_deny";

/// 接收批应答（host-peer `respond-transfer`）：回执表取出通道并写应答——不在册
/// （超时/已终态/ID 未知）返回 false，插件侧据此关闭弹窗
pub(crate) async fn respond_peer_transfer(app: AppHandle, batch_id: String, accepted: bool) -> crate::Result<bool> {
    bedcode_server_peer_net::respond_transfer_for_plugin(peer_ctx(&app), batch_id, accepted).await
}

/// 取消接收批（pending 批视同拒绝；其余按批中止）；返回是否命中
pub(crate) async fn cancel_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::cancel_receiving_for_plugin(peer_ctx(&app), batch_id).await
}

/// 暂停接收批（对端数据供方门控推流；连接保持、断点不丢）
pub(crate) async fn pause_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::peer_engine_receive::pause_peer_receiving(peer_ctx(&app), batch_id).await
}

/// 恢复暂停的接收批（写 Resume 帧续流）
pub(crate) async fn resume_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::peer_engine_receive::resume_peer_receiving(peer_ctx(&app), batch_id).await
}

/// 更新接收策略与询问超时（校验在能力域；运行中节点热生效）。
/// 真入口 = host-peer `set-receive-policy`，设置真源在插件侧
pub(crate) async fn set_peer_receive_policy(app: AppHandle, mode: String, timeout_secs: u64) -> crate::Result<()> {
    bedcode_server_peer_net::set_receive_policy_for_plugin(peer_ctx(&app), mode, timeout_secs).await
}

/// 更换接收落点目录（host-peer `set-download-dir`；None = 恢复缺省落点）
pub(crate) async fn set_peer_download_dir(app: &AppHandle, path: Option<String>) -> crate::Result<()> {
    bedcode_server_peer_net::set_download_dir_for_plugin(peer_ctx(app), path).await
}
