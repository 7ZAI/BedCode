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

use bedcode_host_kit::WasmPluginState;
use bedcode_server_peer_net::plugin_binding;
use bedcode_server_peer_net::plugin_binding::ports::ports;

use crate::plugin::bindings::bedcode;

impl bedcode::plugin::host_peer::Host for WasmPluginState {
    fn dial_peer(&mut self, endpoint_json: String) -> Result<String, String> {
        peer_dial(self, &endpoint_json)
    }

    fn close(&mut self, handle: String) -> Result<bool, String> {
        peer_close(self, &handle)
    }

    fn respond_consent(&mut self, request_id: String, accepted: bool) -> Result<bool, String> {
        peer_respond_consent(self, &request_id, accepted)
    }

    fn list_trusted(&mut self) -> Result<String, String> {
        peer_list_trusted(self)
    }

    fn revoke_trusted(&mut self, node_id: String) -> Result<bool, String> {
        peer_revoke_trusted(self, &node_id)
    }

    fn send_files(&mut self, session: String, paths_json: String) -> Result<String, String> {
        peer_send_files(self, &session, &paths_json)
    }

    fn respond_transfer(&mut self, batch_id: String, accept: bool) -> Result<(), String> {
        peer_respond_transfer(self, &batch_id, accept)
    }

    fn set_receive_policy(&mut self, mode: String, timeout_secs: u64) -> Result<(), String> {
        peer_set_receive_policy(self, &mode, timeout_secs)
    }

    fn pause_transfer(&mut self, batch_id: String) -> Result<(), String> {
        peer_pause_transfer(self, &batch_id)
    }

    fn resume_transfer(&mut self, batch_id: String) -> Result<(), String> {
        peer_resume_transfer(self, &batch_id)
    }

    fn set_shared_roots(&mut self, dirs_json: String) -> Result<(), String> {
        peer_set_shared_roots(self, &dirs_json)
    }

    fn list_shared_roots(&mut self, session: String) -> Result<String, String> {
        peer_list_shared_roots(self, &session)
    }

    fn browse_directory(
        &mut self,
        session: String,
        dir_id: String,
        rel_path: String,
    ) -> Result<String, String> {
        peer_browse_directory(self, &session, &dir_id, &rel_path)
    }

    fn pull_files(
        &mut self,
        session: String,
        dir_id: String,
        files_json: String,
    ) -> Result<u32, String> {
        peer_pull_files(self, &session, &dir_id, &files_json)
    }

    fn set_download_dir(&mut self, path: String) -> Result<(), String> {
        peer_set_download_dir(self, &path)
    }

    fn start_node(&mut self) -> Result<bool, String> {
        peer_start_node(self)
    }

    fn stop_node(&mut self) -> Result<bool, String> {
        peer_stop_node(self)
    }

    fn active_transfers(&mut self) -> Result<String, String> {
        peer_active_transfers(self)
    }

    fn collect_outgoing(&mut self, paths_json: String) -> Result<String, String> {
        peer_collect_outgoing(self, &paths_json)
    }
}


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
