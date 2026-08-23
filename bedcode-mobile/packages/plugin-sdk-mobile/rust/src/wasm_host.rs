//! WASM 插件侧宿主 API 绑定（Component Model 形态，迁移 ticket 04）
//!
//! [`WasmHost`] 以组件 import 后端实现 `host/*` 全部功能 trait，
//! 插件通过这些调用访问宿主能力。编译为 WASM 组件时，调用 wit-bindgen
//! 生成的 import 函数（`crate::wasm::bedcode::plugin::<iface>::<fn>`），
//! 宿主侧由 `wasm_runtime::component` 的 `add_to_linker` 注册的 Host trait 响应。
//!
//! 与旧 ABI（extern "C" + (ptr,len) 内存搬运）的差异：
//! - 内存搬运由绑定层处理，无 alloc/dealloc 配对，杜绝泄漏
//! - 错误经 WIT `result<T, string>` 透传宿主可读消息（旧 ABI 仅 -1 状态码）
//!
//! 插件身份（plugin_id）由宿主侧 Caller state 维护并注入各 import，
//! 插件侧无需持有 —— `WasmHost` 是无状态 unit struct。
//! trait 签名（`host/*` 定义）保持不变，插件业务代码零改动。

use crate::host::{
    ConfigKey, HostBus, HostConfig, HostDatabase, HostError, HostEvents, HostFs,
    HostHttp, HostLog, HostPeer, HostStorage, HostTerminal,
};
use crate::wasm::bedcode::plugin::{
    host_bus, host_config, host_database, host_events, host_fs, host_http,
    host_log, host_peer, host_storage, host_terminal,
};

/// 宿主 API 绑定（WASM 插件侧）
///
/// 无状态 unit struct，通过组件 import 调用宿主注册的 host 接口。
/// 实现了 `host/*` 模块的全部功能 trait（自动获得 `HostApi` 聚合 trait）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WasmHost;

/// 宿主错误 → SDK HostError（WIT `result<T, string>` 的错误串即宿主可读消息）
fn host_err(api: &str, msg: String) -> HostError {
    HostError::custom(-1, format!("{}: {}", api, msg))
}

/// 宿主返回的 JSON 字符串 → serde_json::Value
fn parse_json(api: &str, s: String) -> Result<serde_json::Value, HostError> {
    serde_json::from_str(&s)
        .map_err(|e| HostError::custom(-1, format!("{}: invalid JSON from host: {}", api, e)))
}

#[cfg(all(test, feature = "wasm"))]
mod tests {
    use super::*;

    #[test]
    fn test_host_err_format() {
        // WIT result<T, string> 的错误串 → HostError：code 固定 -1，消息带 API 名前缀
        let e = host_err("db_execute", "permission denied".to_string());
        assert_eq!(e.code, -1);
        assert_eq!(e.message, "db_execute: permission denied");
    }

    #[test]
    fn test_parse_json_valid() {
        let v = parse_json("storage_get", "{\"k\": 1}".to_string()).unwrap();
        assert_eq!(v, serde_json::json!({ "k": 1 }));
    }

    #[test]
    fn test_parse_json_invalid_reports_api() {
        // 解析失败时错误消息必须包含 API 名，插件据此定位宿主侧问题
        let e = parse_json("fs_read", "not json".to_string()).unwrap_err();
        assert_eq!(e.code, -1);
        assert!(e.message.contains("fs_read"), "got: {}", e.message);
        assert!(e.message.contains("invalid JSON from host"));
    }
}

// ==================== HostStorage ====================

impl HostStorage for WasmHost {
    fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, HostError> {
        match host_storage::get(key).map_err(|e| host_err("storage_get", e))? {
            Some(s) => parse_json("storage_get", s).map(Some),
            None => Ok(None),
        }
    }

    fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError> {
        let val_str = serde_json::to_string(value)
            .map_err(|e| HostError::custom(-1, format!("storage_set: serialize failed: {}", e)))?;
        host_storage::set(key, &val_str).map_err(|e| host_err("storage_set", e))
    }

    fn storage_delete(&self, key: &str) -> Result<(), HostError> {
        host_storage::delete(key).map_err(|e| host_err("storage_delete", e))
    }
}

// ==================== HostDatabase ====================

impl HostDatabase for WasmHost {
    fn db_execute(&self, sql: &str) -> Result<i32, HostError> {
        host_database::execute(sql)
            .map(|n| n as i32)
            .map_err(|e| host_err("db_execute", e))
    }

    fn db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError> {
        match host_database::query(sql).map_err(|e| host_err("db_query", e))? {
            Some(s) => parse_json("db_query", s).map(Some),
            None => Ok(None),
        }
    }
}

// ==================== HostTerminal ====================

impl HostTerminal for WasmHost {
    fn terminal_send(&self, session_id: &str, data: &str) -> Result<(), HostError> {
        host_terminal::send(session_id, data).map_err(|e| host_err("terminal_send", e))
    }
}

// ==================== HostEvents ====================

impl HostEvents for WasmHost {
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value) {
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        host_events::emit(event_name, &payload_str);
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), HostError> {
        host_events::notify(title, body).map_err(|e| host_err("notify", e))
    }
}

// ==================== HostHttp ====================

impl HostHttp for WasmHost {
    fn http_fetch(
        &self,
        request: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, HostError> {
        let req_str = serde_json::to_string(request)
            .map_err(|e| HostError::custom(-1, format!("http_fetch: serialize failed: {}", e)))?;
        match host_http::fetch(&req_str).map_err(|e| host_err("http_fetch", e))? {
            Some(s) => parse_json("http_fetch", s).map(Some),
            None => Ok(None),
        }
    }
}

// ==================== HostFs ====================

impl HostFs for WasmHost {
    fn fs_read(&self, path: &str) -> Result<Option<String>, HostError> {
        host_fs::read(path).map_err(|e| host_err("fs_read", e))
    }

    fn fs_write(&self, path: &str, data: &str) -> Result<(), HostError> {
        host_fs::write(path, data).map_err(|e| host_err("fs_write", e))
    }

    fn fs_copy(&self, src: &str, dst: &str) -> Result<(), HostError> {
        host_fs::copy(src, dst).map_err(|e| host_err("fs_copy", e))
    }

    fn fs_delete(&self, path: &str) -> Result<(), HostError> {
        host_fs::delete(path).map_err(|e| host_err("fs_delete", e))
    }

    fn fs_exists(&self, path: &str) -> Result<bool, HostError> {
        host_fs::exists(path).map_err(|e| host_err("fs_exists", e))
    }

    fn fs_request_auth(&self, paths: &[String]) -> Result<bool, HostError> {
        let paths_json = serde_json::to_string(paths).map_err(|e| {
            HostError::custom(-1, format!("fs_request_auth: serialize failed: {}", e))
        })?;
        host_fs::request_auth(&paths_json).map_err(|e| host_err("fs_request_auth", e))
    }

    fn fs_write_media_downloads(
        &self,
        src_path: &str,
        display_name: &str,
        mime_type: &str,
    ) -> Result<(), HostError> {
        host_fs::write_media_downloads(src_path, display_name, mime_type)
            .map_err(|e| host_err("fs_write_media_downloads", e))
    }

    fn fs_save_to_document(
        &self,
        src_path: &str,
        suggested_name: &str,
        mime_type: &str,
    ) -> Result<(), HostError> {
        host_fs::save_to_document(src_path, suggested_name, mime_type)
            .map_err(|e| host_err("fs_save_to_document", e))
    }
}

// ==================== HostLog ====================

impl HostLog for WasmHost {
    fn log_info(&self, message: &str) {
        host_log::info(message);
    }

    fn log_debug(&self, message: &str) {
        host_log::debug(message);
    }

    fn log_warn(&self, message: &str) {
        host_log::warn(message);
    }

    fn log_error(&self, message: &str) {
        host_log::error(message);
    }

    fn mark_plugin_error(&self, error: &str) {
        host_log::mark_plugin_error(error);
    }
}

// ==================== HostBus ====================

impl HostBus for WasmHost {
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), HostError> {
        let payload_str = serde_json::to_string(payload)
            .map_err(|e| HostError::custom(-1, format!("bus_publish: serialize failed: {}", e)))?;
        host_bus::publish(topic, &payload_str).map_err(|e| host_err("bus_publish", e))
    }

    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError> {
        host_bus::subscribe(topic).map_err(|e| host_err("bus_subscribe", e))
    }

    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError> {
        host_bus::unsubscribe(topic).map_err(|e| host_err("bus_unsubscribe", e))
    }
}

// ==================== HostConfig ====================

impl HostConfig for WasmHost {
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError> {
        host_config::get(key.as_str()).map_err(|e| host_err("config_get", e))
    }
}

// ==================== HostPeer ====================

/// 宿主返回的 JSON 字符串 → Value 的统一包装（peer 接口全部 DTO 走此路径）
fn peer_json(api: &str, s: String) -> Result<serde_json::Value, HostError> {
    parse_json(api, s)
}

fn to_json_string(api: &str, value: &serde_json::Value) -> Result<String, HostError> {
    serde_json::to_string(value)
        .map_err(|e| HostError::custom(-1, format!("{api}: serialize failed: {e}")))
}

impl HostPeer for WasmHost {
    fn peer_list_devices(&self) -> Result<serde_json::Value, HostError> {
        peer_json("peer_list_devices", host_peer::list_devices().map_err(|e| host_err("peer_list_devices", e))?)
    }

    fn peer_dial(&self, node_id: &str) -> Result<serde_json::Value, HostError> {
        peer_json("peer_dial", host_peer::dial_peer(node_id).map_err(|e| host_err("peer_dial", e))?)
    }

    fn peer_disconnect(&self, node_id: &str) -> Result<bool, HostError> {
        host_peer::disconnect_peer(node_id).map_err(|e| host_err("peer_disconnect", e))
    }

    fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, HostError> {
        host_peer::respond_consent(request_id, accepted).map_err(|e| host_err("peer_respond_consent", e))
    }

    fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
        peer_json("peer_list_trusted", host_peer::list_trusted().map_err(|e| host_err("peer_list_trusted", e))?)
    }

    fn peer_revoke_trusted(&self, node_id: &str) -> Result<bool, HostError> {
        host_peer::revoke_trusted(node_id).map_err(|e| host_err("peer_revoke_trusted", e))
    }

    fn peer_send_files(&self, node_id: &str, paths: &[String]) -> Result<serde_json::Value, HostError> {
        let paths_json = to_json_string("peer_send_files", &serde_json::to_value(paths).unwrap_or_default())?;
        peer_json("peer_send_files", host_peer::send_files(node_id, &paths_json).map_err(|e| host_err("peer_send_files", e))?)
    }

    fn peer_list_transfers(&self) -> Result<serde_json::Value, HostError> {
        peer_json("peer_list_transfers", host_peer::list_transfers().map_err(|e| host_err("peer_list_transfers", e))?)
    }

    fn peer_cancel_transfer(&self, batch_id: &str) -> Result<bool, HostError> {
        host_peer::cancel_transfer(batch_id).map_err(|e| host_err("peer_cancel_transfer", e))
    }

    fn peer_retry_transfer(&self, batch_id: &str) -> Result<serde_json::Value, HostError> {
        peer_json("peer_retry_transfer", host_peer::retry_transfer(batch_id).map_err(|e| host_err("peer_retry_transfer", e))?)
    }

    fn peer_clear_transfer_history(&self) -> Result<u32, HostError> {
        host_peer::clear_transfer_history().map_err(|e| host_err("peer_clear_transfer_history", e))
    }

    fn peer_list_receiving(&self) -> Result<serde_json::Value, HostError> {
        peer_json("peer_list_receiving", host_peer::list_receiving().map_err(|e| host_err("peer_list_receiving", e))?)
    }

    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> Result<(), HostError> {
        host_peer::respond_transfer(batch_id, accept).map_err(|e| host_err("peer_respond_transfer", e))
    }

    fn peer_cancel_receiving(&self, batch_id: &str) -> Result<bool, HostError> {
        host_peer::cancel_receiving(batch_id).map_err(|e| host_err("peer_cancel_receiving", e))
    }

    fn peer_clear_receiving_history(&self) -> Result<u32, HostError> {
        host_peer::clear_receiving_history().map_err(|e| host_err("peer_clear_receiving_history", e))
    }

    fn peer_get_receive_settings(&self) -> Result<serde_json::Value, HostError> {
        peer_json("peer_get_receive_settings", host_peer::get_receive_settings().map_err(|e| host_err("peer_get_receive_settings", e))?)
    }

    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> Result<(), HostError> {
        host_peer::set_receive_policy(mode, timeout_secs).map_err(|e| host_err("peer_set_receive_policy", e))
    }

    fn peer_list_shared_directories(&self) -> Result<serde_json::Value, HostError> {
        peer_json(
            "peer_list_shared_directories",
            host_peer::list_shared_directories().map_err(|e| host_err("peer_list_shared_directories", e))?,
        )
    }

    fn peer_remove_shared_directory(&self, id: &str) -> Result<bool, HostError> {
        host_peer::remove_shared_directory(id).map_err(|e| host_err("peer_remove_shared_directory", e))
    }

    fn peer_add_shared_directory(
        &self,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, HostError> {
        let request_json = to_json_string("peer_add_shared_directory", request)?;
        peer_json(
            "peer_add_shared_directory",
            host_peer::add_shared_directory(&request_json)
                .map_err(|e| host_err("peer_add_shared_directory", e))?,
        )
    }

    fn peer_list_shared_roots(&self, node_id: &str) -> Result<serde_json::Value, HostError> {
        peer_json(
            "peer_list_shared_roots",
            host_peer::list_shared_roots(node_id).map_err(|e| host_err("peer_list_shared_roots", e))?,
        )
    }

    fn peer_browse_directory(
        &self,
        node_id: &str,
        dir_id: &str,
        rel_path: &str,
    ) -> Result<serde_json::Value, HostError> {
        peer_json(
            "peer_browse_directory",
            host_peer::browse_directory(node_id, dir_id, rel_path)
                .map_err(|e| host_err("peer_browse_directory", e))?,
        )
    }

    fn peer_pull_files(
        &self,
        node_id: &str,
        dir_id: &str,
        files: &[serde_json::Value],
    ) -> Result<u32, HostError> {
        let files_json = to_json_string("peer_pull_files", &serde_json::to_value(files).unwrap_or_default())?;
        host_peer::pull_files(node_id, dir_id, &files_json).map_err(|e| host_err("peer_pull_files", e))
    }

    fn peer_pick_files(&self) -> Result<Vec<String>, HostError> {
        let v = peer_json("peer_pick_files", host_peer::pick_files().map_err(|e| host_err("peer_pick_files", e))?)?;
        serde_json::from_value(v)
            .map_err(|e| HostError::custom(-1, format!("peer_pick_files: invalid JSON from host: {e}")))
    }

    fn peer_pick_folder(&self) -> Result<String, HostError> {
        host_peer::pick_folder().map_err(|e| host_err("peer_pick_folder", e))
    }

    fn peer_set_download_dir(&self, path: &str) -> Result<(), HostError> {
        host_peer::set_download_dir(path).map_err(|e| host_err("peer_set_download_dir", e))
    }
}
