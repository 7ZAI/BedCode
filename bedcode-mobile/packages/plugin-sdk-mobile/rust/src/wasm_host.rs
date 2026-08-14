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
    ConfigKey, HostBus, HostConfig, HostDatabase, HostError, HostEvents, HostFileService, HostFs,
    HostHttp, HostLog, HostStorage, HostTerminal, HostTransfer,
};
use crate::types::{MountOptions, MountResult, PeerFileService, TransferRequest};
use crate::wasm::bedcode::plugin::{
    host_bus, host_config, host_database, host_events, host_file_service, host_fs, host_http,
    host_log, host_storage, host_terminal, host_transfer,
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

// ==================== HostFileService ====================

impl HostFileService for WasmHost {
    fn filesrv_mount(&self, options: &MountOptions) -> Result<MountResult, HostError> {
        let opts_str = serde_json::to_string(options).map_err(|e| {
            HostError::custom(-1, format!("filesrv_mount: serialize options failed: {}", e))
        })?;
        let json_str = host_file_service::mount(&opts_str)
            .map_err(|e| host_err("filesrv_mount", e))?;
        serde_json::from_str(&json_str).map_err(|e| {
            HostError::custom(-1, format!("filesrv_mount: invalid JSON from host: {}", e))
        })
    }

    fn filesrv_unmount(&self, mount_path: &str) -> Result<(), HostError> {
        host_file_service::unmount(mount_path).map_err(|e| host_err("filesrv_unmount", e))
    }

    fn filesrv_update_roots(&self, mount_path: &str, roots: &[String]) -> Result<(), HostError> {
        let roots_str = serde_json::to_string(roots).unwrap_or_else(|_| "[]".to_string());
        host_file_service::update_roots(mount_path, &roots_str)
            .map_err(|e| host_err("filesrv_update_roots", e))
    }

    fn filesrv_get_peer(&self, peer_id: &str) -> Result<Option<PeerFileService>, HostError> {
        match host_file_service::get_peer(peer_id)
            .map_err(|e| host_err("filesrv_get_peer", e))?
        {
            Some(s) => serde_json::from_str(&s).map(Some).map_err(|e| {
                HostError::custom(-1, format!("filesrv_get_peer: invalid JSON from host: {}", e))
            }),
            None => Ok(None),
        }
    }

    fn filesrv_query_peer(&self, peer_id: &str) -> Result<(), HostError> {
        host_file_service::query_peer(peer_id).map_err(|e| host_err("filesrv_query_peer", e))
    }

    fn filesrv_approve_transfer(&self, batch_id: &str) -> Result<(), HostError> {
        host_file_service::approve_transfer(batch_id)
            .map_err(|e| host_err("filesrv_approve_transfer", e))
    }

    fn filesrv_reject_transfer(&self, batch_id: &str) -> Result<(), HostError> {
        host_file_service::reject_transfer(batch_id)
            .map_err(|e| host_err("filesrv_reject_transfer", e))
    }

    fn filesrv_set_approval_timeout(&self, mount_path: &str, seconds: u64) -> Result<(), HostError> {
        host_file_service::set_approval_timeout(mount_path, seconds)
            .map_err(|e| host_err("filesrv_set_approval_timeout", e))
    }

    fn filesrv_cancel_receiving(&self, session_id: &str) -> Result<(), HostError> {
        host_file_service::cancel_receiving(session_id)
            .map_err(|e| host_err("filesrv_cancel_receiving", e))
    }
}

// ==================== HostTransfer ====================

impl HostTransfer for WasmHost {
    fn transfer_start(&self, request: &TransferRequest) -> Result<String, HostError> {
        let req_str = serde_json::to_string(request).map_err(|e| {
            HostError::custom(-1, format!("transfer_start: serialize request failed: {}", e))
        })?;
        host_transfer::start(&req_str).map_err(|e| host_err("transfer_start", e))
    }

    fn transfer_cancel(&self, task_id: &str) -> Result<(), HostError> {
        host_transfer::cancel(task_id).map_err(|e| host_err("transfer_cancel", e))
    }
}
