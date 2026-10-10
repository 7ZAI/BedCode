//! host-peer 域 —— 薄转发（域逻辑在共享能力域 crate）
//!
//! 19 条 `host-peer` 原语的机制（session 句柄表 / 属主仲裁 / 数据面自动重拨 /
//! 载荷校验 / 权限门位置）全在 `packages/bedcode-server-peer-net::plugin_binding`
//! （WIT-无关域函数 + [`PeerPorts`] 端口，无 feature 门控）；本层只做
//! 「取已装配端口 → 转调 crate → 按 WIT `result` 形状返回」。
//!
//! **端口装配**：移动宿主 `bedcode-mobile/src-tauri/src/plugin/peer_ports.rs`
//! 开机期调 `plugin_binding::install_ports` 装入（权限判定源 = 宿主 PluginManager /
//! 引擎上下文 = `crate::peer_net::peer_ctx` / 同步桥 = 宿主 runtime_util）；
//! 取不到端口即 panic（fail-visible，见 crate `ports::ports` 文档）。
//!
//! **与桌面形态的关系**：桌面同款实现对住在 crate 的 `common-host` 绑定层
//! （`impl Host` 调同一批 19 函数）——两端共享域逻辑，差异只剩端口实现。
//!
//! [`PeerPorts`]: bedcode_server_peer_net::plugin_binding::ports::PeerPorts

use super::super::WasmPluginState;
use bedcode_server_peer_net::plugin_binding;
use bedcode_server_peer_net::plugin_binding::ports::ports;

pub(crate) fn peer_dial(state: &WasmPluginState, endpoint_json: &str) -> Result<String, String> {
    plugin_binding::peer_dial(&ports(), &state.plugin_id, endpoint_json)
}

pub(crate) fn peer_close(state: &WasmPluginState, handle: &str) -> Result<bool, String> {
    plugin_binding::peer_close(&ports(), &state.plugin_id, handle)
}

pub(crate) fn peer_respond_consent(
    state: &WasmPluginState,
    request_id: &str,
    accepted: bool,
) -> Result<bool, String> {
    plugin_binding::peer_respond_consent(&ports(), &state.plugin_id, request_id, accepted)
}

pub(crate) fn peer_list_trusted(state: &WasmPluginState) -> Result<String, String> {
    plugin_binding::peer_list_trusted(&ports(), &state.plugin_id)
}

pub(crate) fn peer_revoke_trusted(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    plugin_binding::peer_revoke_trusted(&ports(), &state.plugin_id, node_id)
}

pub(crate) fn peer_send_files(
    state: &WasmPluginState,
    session: &str,
    paths_json: &str,
) -> Result<String, String> {
    plugin_binding::peer_send_files(&ports(), &state.plugin_id, session, paths_json)
}

pub(crate) fn peer_respond_transfer(
    state: &WasmPluginState,
    batch_id: &str,
    accept: bool,
) -> Result<(), String> {
    plugin_binding::peer_respond_transfer(&ports(), &state.plugin_id, batch_id, accept)
}

pub(crate) fn peer_set_receive_policy(
    state: &WasmPluginState,
    mode: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    plugin_binding::peer_set_receive_policy(&ports(), &state.plugin_id, mode, timeout_secs)
}

pub(crate) fn peer_pause_transfer(state: &WasmPluginState, batch_id: &str) -> Result<(), String> {
    plugin_binding::peer_pause_transfer(&ports(), &state.plugin_id, batch_id)
}

pub(crate) fn peer_resume_transfer(state: &WasmPluginState, batch_id: &str) -> Result<(), String> {
    plugin_binding::peer_resume_transfer(&ports(), &state.plugin_id, batch_id)
}

pub(crate) fn peer_set_shared_roots(state: &WasmPluginState, dirs_json: &str) -> Result<(), String> {
    plugin_binding::peer_set_shared_roots(&ports(), &state.plugin_id, dirs_json)
}

pub(crate) fn peer_list_shared_roots(state: &WasmPluginState, session: &str) -> Result<String, String> {
    plugin_binding::peer_list_shared_roots(&ports(), &state.plugin_id, session)
}

pub(crate) fn peer_browse_directory(
    state: &WasmPluginState,
    session: &str,
    dir_id: &str,
    rel_path: &str,
) -> Result<String, String> {
    plugin_binding::peer_browse_directory(&ports(), &state.plugin_id, session, dir_id, rel_path)
}

pub(crate) fn peer_pull_files(
    state: &WasmPluginState,
    session: &str,
    dir_id: &str,
    files_json: &str,
) -> Result<u32, String> {
    plugin_binding::peer_pull_files(&ports(), &state.plugin_id, session, dir_id, files_json)
}

pub(crate) fn peer_set_download_dir(state: &WasmPluginState, path: &str) -> Result<(), String> {
    plugin_binding::peer_set_download_dir(&ports(), &state.plugin_id, path)
}

pub(crate) fn peer_start_node(state: &WasmPluginState) -> Result<bool, String> {
    plugin_binding::peer_start_node(&ports(), &state.plugin_id)
}

pub(crate) fn peer_stop_node(state: &WasmPluginState) -> Result<bool, String> {
    plugin_binding::peer_stop_node(&ports(), &state.plugin_id)
}

pub(crate) fn peer_active_transfers(state: &WasmPluginState) -> Result<String, String> {
    plugin_binding::peer_active_transfers(&ports(), &state.plugin_id)
}

pub(crate) fn peer_collect_outgoing(state: &WasmPluginState, paths_json: &str) -> Result<String, String> {
    plugin_binding::peer_collect_outgoing(&ports(), &state.plugin_id, paths_json)
}
