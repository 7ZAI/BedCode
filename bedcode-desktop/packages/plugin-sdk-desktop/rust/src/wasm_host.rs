//! WASM 插件侧宿主 API 绑定（Component Model 形态，迁移阶段 B）
//!
//! [`WasmHost`] 以组件 import 后端实现 `host/*` 全部功能 trait，
//! 插件通过这些调用访问宿主能力。编译为 WASM 组件时，调用 wit-bindgen
//! 生成的 import 函数（`crate::wasm::bedcode::plugin::<iface>::<fn>`），
//! 宿主侧由 `wasm_runtime::component` 的 `add_to_linker` 注册的 Host trait 响应。
//!
//! 与旧 ABI（extern "C" + (ptr,len) 内存搬运）的差异：
//! - 内存搬运由绑定层处理，无 alloc/dealloc 配对，杜绝泄漏
//! - 错误经 WIT `result<T, string>` 透传宿主可读消息（旧 ABI 仅 -1 状态码）
//! - 日志不再附带插件调用点 file/line（WIT host-log 暂无该通道，见契约注释）
//!
//! 插件身份（plugin_id）由宿主侧 Caller state 维护并注入各 import，
//! 插件侧无需持有 —— `WasmHost` 是无状态 unit struct。
//! trait 签名（`host/*` 定义）保持不变，插件业务代码零改动。

use crate::host::{
    ConfigKey, FsDirEntry, FsStat, HostApp, HostAuth, HostBus, HostConfig, HostConnection, HostDatabase, HostError,
    HostEvents, HostFs, HostHttp, HostLog, HostMdns, HostPeer, HostPlatform, HostPluginDatabase,
    HostCrypto, HostProcess, HostPty, HostStorage, HostTask, HostWebsocket,
    CryptoKeypair, ProcessSyncResult, PtyRingFetch,
};
use crate::wasm::bedcode::plugin::{
    host_app, host_auth, host_bus, host_config, host_database, host_events, host_fs, host_http,
    host_crypto, host_log, host_mdns, host_peer, host_platform, host_plugin_database, host_process, host_pty,
    host_connection, host_storage, host_task, host_timer, host_websocket,
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

#[cfg(test)]
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
        let e = parse_json("session_get", "not json".to_string()).unwrap_err();
        assert_eq!(e.code, -1);
        assert!(e.message.contains("session_get"), "got: {}", e.message);
        assert!(e.message.contains("invalid JSON from host"));
    }
}

// ==================== HostAuth（v15 secret-store + v18 认证记录面） ====================

impl HostAuth for WasmHost {
    fn auth_secret_get(&self, key: &str) -> Result<Option<String>, HostError> {
        host_auth::secret_get(key).map_err(|e| host_err("auth_secret_get", e))
    }

    fn auth_secret_set(&self, key: &str, value: &str) -> Result<(), HostError> {
        host_auth::secret_set(key, value).map_err(|e| host_err("auth_secret_set", e))
    }

    fn auth_secret_delete(&self, key: &str) -> Result<(), HostError> {
        host_auth::secret_delete(key).map_err(|e| host_err("auth_secret_delete", e))
    }

    fn auth_secret_keys(&self) -> Result<Vec<String>, HostError> {
        host_auth::secret_keys().map_err(|e| host_err("auth_secret_keys", e))
    }

    fn auth_setting_set(&self, key: &str, value: &str) -> Result<(), HostError> {
        host_auth::auth_setting_set(key, value).map_err(|e| host_err("auth_setting_set", e))
    }

    // ==================== v19 保留面（v24 修订语义：公钥托管在 plugin_secrets） ====================

    fn auth_biometric_credential_bound(&self, fingerprint: &str) -> Result<bool, HostError> {
        host_auth::biometric_credential_bound(fingerprint)
            .map_err(|e| host_err("auth_biometric_credential_bound", e))
    }

    fn auth_biometric_verify_signature(
        &self,
        fingerprint: &str,
        message: &str,
        signature: &str,
    ) -> Result<bool, HostError> {
        host_auth::biometric_verify_signature(fingerprint, message, signature)
            .map_err(|e| host_err("auth_biometric_verify_signature", e))
    }

    fn auth_link_identity_parts(&self) -> Result<Option<serde_json::Value>, HostError> {
        host_auth::link_identity_parts()
            .map_err(|e| host_err("auth_link_identity_parts", e))
            .and_then(|v| match v {
                Some(s) => parse_json("auth_link_identity_parts", s).map(Some),
                None => Ok(None),
            })
    }

    fn auth_biometric_credential_bind(&self, fingerprint: &str, public_key: &str) -> Result<bool, HostError> {
        host_auth::biometric_credential_bind(fingerprint, public_key)
            .map_err(|e| host_err("auth_biometric_credential_bind", e))
    }

    fn auth_device_token_issue(&self, sub: &str, device_name: &str, fingerprint: &str) -> Result<String, HostError> {
        host_auth::device_token_issue(sub, device_name, fingerprint)
            .map_err(|e| host_err("auth_device_token_issue", e))
    }

    fn auth_device_token_verify(&self, token: &str) -> Result<String, HostError> {
        host_auth::device_token_verify(token).map_err(|e| host_err("auth_device_token_verify", e))
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

// ==================== HostDatabase / HostPluginDatabase ====================

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

    fn db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError> {
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        host_database::execute_params(sql, &params_str)
            .map(|n| n as i32)
            .map_err(|e| host_err("db_execute_params", e))
    }

    fn db_query_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Option<serde_json::Value>, HostError> {
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        match host_database::query_params(sql, &params_str)
            .map_err(|e| host_err("db_query_params", e))?
        {
            Some(s) => parse_json("db_query_params", s).map(Some),
            None => Ok(None),
        }
    }

    fn db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError> {
        let sqls_str = serde_json::to_string(sqls).map_err(|e| {
            HostError::custom(-1, format!("db_execute_batch: serialize failed: {}", e))
        })?;
        host_database::execute_batch(&sqls_str)
            .map(|n| n as i32)
            .map_err(|e| host_err("db_execute_batch", e))
    }
}

impl HostPluginDatabase for WasmHost {
    fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError> {
        host_plugin_database::execute(sql)
            .map(|n| n as i32)
            .map_err(|e| host_err("plugin_db_execute", e))
    }

    fn plugin_db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError> {
        match host_plugin_database::query(sql).map_err(|e| host_err("plugin_db_query", e))? {
            Some(s) => parse_json("plugin_db_query", s).map(Some),
            None => Ok(None),
        }
    }

    fn plugin_db_execute_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<i32, HostError> {
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        host_plugin_database::execute_params(sql, &params_str)
            .map(|n| n as i32)
            .map_err(|e| host_err("plugin_db_execute_params", e))
    }

    fn plugin_db_query_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Option<serde_json::Value>, HostError> {
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        match host_plugin_database::query_params(sql, &params_str)
            .map_err(|e| host_err("plugin_db_query_params", e))?
        {
            Some(s) => parse_json("plugin_db_query_params", s).map(Some),
            None => Ok(None),
        }
    }

    fn plugin_db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError> {
        let sqls_str = serde_json::to_string(sqls).map_err(|e| {
            HostError::custom(
                -1,
                format!("plugin_db_execute_batch: serialize failed: {}", e),
            )
        })?;
        host_plugin_database::execute_batch(&sqls_str)
            .map(|n| n as i32)
            .map_err(|e| host_err("plugin_db_execute_batch", e))
    }
}

// ==================== HostTerminal（v27 已退役） ====================

// `host-terminal` 整 interface 在 ABI v27 删除（票 10）：`send` 是「宿主替插件往
// 交互终端注入按键」的业务面入口，零生产消费者（属主判定查已清空的内核登记）。
// 插件写自家会话输入走自有命令通道（`session.input` → `session::input_via_pty`）。

// ==================== HostSession（v27 已退役） ====================

// `host-session` 整 interface 在 ABI v27 删除（票 10）：会话真源在
// `com.bedcode.terminal-session` 登记域，宿主侧不再有「会话」原语域。
// 插件要用会话能力一律经 `host-pty`（PTY 引擎）与自家登记域。

// ==================== HostConnection ====================

impl HostConnection for WasmHost {
    /// 走 `host-connection`（票 04）：这份宿主 server 的连接事实
    /// 不随会话原语域退役；权限判据是 `connection:read`。
    fn connections_list(&self) -> Result<serde_json::Value, HostError> {
        let raw = host_connection::connections_list().map_err(|e| host_err("connections_list", e))?;
        parse_json("connections_list", raw)
    }
}

// ==================== HostTimer ====================

impl crate::host::HostTimer for WasmHost {
    fn timer_register(&self, interval_secs: u64, command: &str) -> Result<(), HostError> {
        host_timer::register(interval_secs, command).map_err(|e| host_err("timer_register", e))
    }
}

// ==================== HostProcess ====================

impl HostProcess for WasmHost {
    fn process_run(&self, request_json: &str) -> Result<String, HostError> {
        host_process::run(request_json).map_err(|e| host_err("process_run", e))
    }

    fn process_kill(&self, run_id: &str) -> Result<(), HostError> {
        host_process::kill(run_id).map_err(|e| host_err("process_kill", e))
    }

    fn process_run_sync(&self, request_json: &str) -> Result<ProcessSyncResult, HostError> {
        let json = host_process::run_sync(request_json).map_err(|e| host_err("process_run_sync", e))?;
        serde_json::from_str(&json)
            .map_err(|e| HostError::custom(-1, format!("process_run_sync: decode failed: {}", e)))
    }
}

// ==================== HostTask ====================

impl HostTask for WasmHost {
    fn execute_batch(&self, plan_json: &str) -> Result<String, HostError> {
        host_task::execute_batch(plan_json).map_err(|e| host_err("execute_batch", e))
    }

    fn submit(&self, plan_json: &str) -> Result<String, HostError> {
        host_task::submit(plan_json).map_err(|e| host_err("submit", e))
    }

    fn task_status(&self, job_id: &str) -> Result<Option<String>, HostError> {
        host_task::status(job_id).map_err(|e| host_err("task_status", e))
    }

    fn cancel(&self, job_id: &str) -> Result<bool, HostError> {
        host_task::cancel(job_id).map_err(|e| host_err("cancel", e))
    }

    fn list_jobs(&self) -> Result<String, HostError> {
        host_task::list_jobs().map_err(|e| host_err("list_jobs", e))
    }
}

// ==================== HostApp ====================

impl HostApp for WasmHost {
    fn cli_install(&self, file_name: &str, bin_dir: &str) -> Result<String, HostError> {
        let payload = serde_json::json!({ "file_name": file_name, "bin_dir": bin_dir }).to_string();
        host_app::install_cli(&payload).map_err(|e| host_err("cli_install", e))
    }

    fn cli_uninstall(&self, file_name: &str, bin_dir: &str) -> Result<(), HostError> {
        let payload = serde_json::json!({ "file_name": file_name, "bin_dir": bin_dir }).to_string();
        host_app::uninstall_cli(&payload).map_err(|e| host_err("cli_uninstall", e))
    }

    fn plugin_resource_dir(&self) -> Result<String, HostError> {
        host_app::plugin_resource_dir().map_err(|e| host_err("plugin_resource_dir", e))
    }
}

// ==================== HostEvents ====================

impl HostEvents for WasmHost {
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value) {
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        host_events::emit(event_name, &payload_str);
    }

    fn broadcast_sync(&self, event: &crate::events::SyncEvent) {
        // SyncEvent 的 serde 表示即线协议（`{"type": <snake_case>, "data": {…}}`，
        // 与出站 SyncPayload 同构），宿主反序列化为同一类型后不再改写格式
        let payload_str = serde_json::to_string(event).unwrap_or_default();
        host_events::broadcast_sync(&payload_str);
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

    // ==================== v19 追加（票 03 文件浏览域） ====================

    fn fs_read_dir(&self, path: &str) -> Result<Vec<FsDirEntry>, HostError> {
        let json = host_fs::read_dir(path).map_err(|e| host_err("fs_read_dir", e))?;
        serde_json::from_str(&json)
            .map_err(|e| HostError::custom(-1, format!("fs_read_dir: decode failed: {}", e)))
    }

    fn fs_canonicalize(&self, path: &str) -> Result<Option<String>, HostError> {
        host_fs::canonicalize(path).map_err(|e| host_err("fs_canonicalize", e))
    }

    fn fs_stat(&self, path: &str) -> Result<Option<FsStat>, HostError> {
        let json = match host_fs::stat(path).map_err(|e| host_err("fs_stat", e))? {
            Some(json) => json,
            None => return Ok(None),
        };
        serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("fs_stat: decode failed: {}", e)))
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

    fn bus_publish_binary(&self, topic: &str, payload: &[u8]) -> Result<(), HostError> {
        host_bus::publish_binary(topic, payload).map_err(|e| host_err("bus_publish_binary", e))
    }

    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError> {
        host_bus::subscribe(topic).map_err(|e| host_err("bus_subscribe", e))
    }

    fn bus_subscribe_binary(&self, topic: &str) -> Result<(), HostError> {
        host_bus::subscribe_binary(topic).map_err(|e| host_err("bus_subscribe_binary", e))
    }

    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError> {
        host_bus::unsubscribe(topic).map_err(|e| host_err("bus_unsubscribe", e))
    }
}

// ==================== HostConfig ====================

impl HostConfig for WasmHost {
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError> {
        // 宿主对不可用的配置项返回 Err（如 home_dir 解析失败），
        // 语义为"无此配置"而非调用错误（与 core ABI 的 -1 语义一致）；
        // 错误内容记录在宿主日志
        Ok(host_config::get(key.as_str()).ok().flatten())
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
    fn peer_dial(&self, endpoint: &serde_json::Value) -> Result<String, HostError> {
        let endpoint_json = to_json_string("peer_dial", endpoint)?;
        host_peer::dial_peer(&endpoint_json).map_err(|e| host_err("peer_dial", e))
    }

    fn peer_close(&self, handle: &str) -> Result<bool, HostError> {
        host_peer::close(handle).map_err(|e| host_err("peer_close", e))
    }

    fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, HostError> {
        host_peer::respond_consent(request_id, accepted)
            .map_err(|e| host_err("peer_respond_consent", e))
    }

    fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
        peer_json(
            "peer_list_trusted",
            host_peer::list_trusted().map_err(|e| host_err("peer_list_trusted", e))?,
        )
    }

    fn peer_revoke_trusted(&self, node_id: &str) -> Result<bool, HostError> {
        host_peer::revoke_trusted(node_id).map_err(|e| host_err("peer_revoke_trusted", e))
    }

    fn peer_send_files(
        &self,
        session: &str,
        paths: &[serde_json::Value],
    ) -> Result<String, HostError> {
        let paths_json = to_json_string(
            "peer_send_files",
            &serde_json::to_value(paths).unwrap_or_default(),
        )?;
        // 返回值已收窄为传输句柄字符串（Phase 4），不再包一层 JSON
        host_peer::send_files(session, &paths_json).map_err(|e| host_err("peer_send_files", e))
    }

    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> Result<(), HostError> {
        host_peer::respond_transfer(batch_id, accept)
            .map_err(|e| host_err("peer_respond_transfer", e))
    }

    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> Result<(), HostError> {
        host_peer::set_receive_policy(mode, timeout_secs)
            .map_err(|e| host_err("peer_set_receive_policy", e))
    }

    fn peer_pause_transfer(&self, batch_id: &str) -> Result<(), HostError> {
        host_peer::pause_transfer(batch_id).map_err(|e| host_err("peer_pause_transfer", e))
    }

    fn peer_resume_transfer(&self, batch_id: &str) -> Result<(), HostError> {
        host_peer::resume_transfer(batch_id).map_err(|e| host_err("peer_resume_transfer", e))
    }

    fn peer_resume_all_transfers(&self) -> Result<u32, HostError> {
        host_peer::resume_all_transfers().map_err(|e| host_err("peer_resume_all_transfers", e))
    }

    fn peer_set_shared_roots(&self, dirs: &[serde_json::Value]) -> Result<(), HostError> {
        let dirs_json = to_json_string(
            "peer_set_shared_roots",
            &serde_json::to_value(dirs).unwrap_or_default(),
        )?;
        host_peer::set_shared_roots(&dirs_json).map_err(|e| host_err("peer_set_shared_roots", e))
    }

    fn peer_list_shared_roots(&self, session: &str) -> Result<serde_json::Value, HostError> {
        peer_json(
            "peer_list_shared_roots",
            host_peer::list_shared_roots(session)
                .map_err(|e| host_err("peer_list_shared_roots", e))?,
        )
    }

    fn peer_browse_directory(
        &self,
        session: &str,
        dir_id: &str,
        rel_path: &str,
    ) -> Result<serde_json::Value, HostError> {
        peer_json(
            "peer_browse_directory",
            host_peer::browse_directory(session, dir_id, rel_path)
                .map_err(|e| host_err("peer_browse_directory", e))?,
        )
    }

    fn peer_pull_files(
        &self,
        session: &str,
        dir_id: &str,
        files: &[serde_json::Value],
    ) -> Result<u32, HostError> {
        let files_json = to_json_string(
            "peer_pull_files",
            &serde_json::to_value(files).unwrap_or_default(),
        )?;
        host_peer::pull_files(session, dir_id, &files_json)
            .map_err(|e| host_err("peer_pull_files", e))
    }

    fn peer_set_download_dir(&self, path: &str) -> Result<(), HostError> {
        host_peer::set_download_dir(path).map_err(|e| host_err("peer_set_download_dir", e))
    }

    fn peer_start_node(&self) -> Result<bool, HostError> {
        host_peer::start_node().map_err(|e| host_err("peer_start_node", e))
    }

    fn peer_stop_node(&self) -> Result<bool, HostError> {
        host_peer::stop_node().map_err(|e| host_err("peer_stop_node", e))
    }
}

// ==================== host-mdns（ADR 0022 v2）====================

impl HostMdns for WasmHost {
    fn mdns_browse(&self, service_type: &str) -> Result<String, HostError> {
        host_mdns::browse(service_type).map_err(|e| host_err("mdns_browse", e))
    }

    fn mdns_stop_browse(&self, browser_id: &str) -> Result<bool, HostError> {
        host_mdns::stop_browse(browser_id).map_err(|e| host_err("mdns_stop_browse", e))
    }

    fn mdns_advertise(&self, config_json: &str) -> Result<String, HostError> {
        host_mdns::advertise(config_json).map_err(|e| host_err("mdns_advertise", e))
    }

    fn mdns_stop_advertise(&self, advertise_id: &str) -> Result<bool, HostError> {
        host_mdns::stop_advertise(advertise_id).map_err(|e| host_err("mdns_stop_advertise", e))
    }

    fn mdns_is_advertising(&self, advertise_id: &str) -> Result<bool, HostError> {
        host_mdns::is_advertising(advertise_id).map_err(|e| host_err("mdns_is_advertising", e))
    }
}

// ==================== host-websocket（ABI v14）====================

impl HostWebsocket for WasmHost {
    fn ws_connect(&self, config_json: &str) -> Result<String, HostError> {
        host_websocket::connect(config_json).map_err(|e| host_err("ws_connect", e))
    }

    fn ws_send_text(&self, handle: &str, text: &str) -> Result<(), HostError> {
        host_websocket::send_text(handle, text).map_err(|e| host_err("ws_send_text", e))
    }

    fn ws_send_binary(&self, handle: &str, payload: &[u8]) -> Result<(), HostError> {
        host_websocket::send_binary(handle, payload).map_err(|e| host_err("ws_send_binary", e))
    }

    fn ws_close(&self, handle: &str, close_json: &str) -> Result<bool, HostError> {
        host_websocket::close(handle, close_json).map_err(|e| host_err("ws_close", e))
    }

    fn ws_is_connected(&self, handle: &str) -> Result<bool, HostError> {
        host_websocket::is_connected(handle).map_err(|e| host_err("ws_is_connected", e))
    }

    fn ws_register_endpoint(&self, config_json: &str) -> Result<String, HostError> {
        host_websocket::register_endpoint(config_json)
            .map_err(|e| host_err("ws_register_endpoint", e))
    }

    fn ws_send_text_to_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        text: &str,
    ) -> Result<(), HostError> {
        host_websocket::send_text_to_client(endpoint_id, client_id, text)
            .map_err(|e| host_err("ws_send_text_to_client", e))
    }

    fn ws_send_binary_to_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        payload: &[u8],
    ) -> Result<(), HostError> {
        host_websocket::send_binary_to_client(endpoint_id, client_id, payload)
            .map_err(|e| host_err("ws_send_binary_to_client", e))
    }

    fn ws_broadcast_text(&self, endpoint_id: &str, text: &str) -> Result<u32, HostError> {
        host_websocket::broadcast_text(endpoint_id, text)
            .map_err(|e| host_err("ws_broadcast_text", e))
    }

    fn ws_broadcast_binary(&self, endpoint_id: &str, payload: &[u8]) -> Result<u32, HostError> {
        host_websocket::broadcast_binary(endpoint_id, payload)
            .map_err(|e| host_err("ws_broadcast_binary", e))
    }

    fn ws_close_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        close_json: &str,
    ) -> Result<bool, HostError> {
        host_websocket::close_client(endpoint_id, client_id, close_json)
            .map_err(|e| host_err("ws_close_client", e))
    }

    fn ws_unregister_endpoint(&self, endpoint_id: &str) -> Result<bool, HostError> {
        host_websocket::unregister_endpoint(endpoint_id)
            .map_err(|e| host_err("ws_unregister_endpoint", e))
    }

    fn ws_list_clients(&self, endpoint_id: &str) -> Result<String, HostError> {
        host_websocket::list_clients(endpoint_id).map_err(|e| host_err("ws_list_clients", e))
    }

    fn ws_list_endpoints(&self) -> Result<String, HostError> {
        host_websocket::list_endpoints().map_err(|e| host_err("ws_list_endpoints", e))
    }

    fn ws_connection_context(&self, endpoint_id: &str, client_id: &str) -> Result<String, HostError> {
        host_websocket::connection_context(endpoint_id, client_id)
            .map_err(|e| host_err("ws_connection_context", e))
    }
}

// ==================== host-pty（ABI v16 插件私有伪终端）====================

impl HostPty for WasmHost {
    fn pty_spawn(&self, config_json: &str) -> Result<String, HostError> {
        host_pty::spawn(config_json).map_err(|e| host_err("pty_spawn", e))
    }

    fn pty_write(&self, pty_id: &str, data: &[u8]) -> Result<(), HostError> {
        host_pty::write(pty_id, data).map_err(|e| host_err("pty_write", e))
    }

    fn pty_resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), HostError> {
        host_pty::resize(pty_id, cols, rows).map_err(|e| host_err("pty_resize", e))
    }

    fn pty_kill(&self, pty_id: &str) -> Result<(), HostError> {
        host_pty::kill(pty_id).map_err(|e| host_err("pty_kill", e))
    }

    fn pty_ring_fetch(
        &self,
        pty_id: &str,
        from_offset: u64,
        max_bytes: u32,
    ) -> Result<Option<PtyRingFetch>, HostError> {
        host_pty::ring_fetch(pty_id, from_offset, max_bytes)
            .map(|result| {
                result.map(|r| PtyRingFetch {
                    data: r.data,
                    next_offset: r.next_offset,
                    truncated: r.truncated,
                })
            })
            .map_err(|e| host_err("pty_ring_fetch", e))
    }

    fn pty_is_running(&self, pty_id: &str) -> Result<bool, HostError> {
        host_pty::is_running(pty_id).map_err(|e| host_err("pty_is_running", e))
    }
}

// ==================== host-crypto（v26 宿主加密引擎原语面） ====================

impl HostCrypto for WasmHost {
    fn aead_encrypt(
        &self,
        algorithm: &str,
        key: &[u8],
        nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, HostError> {
        host_crypto::aead_encrypt(algorithm, key, nonce, plaintext, aad)
            .map_err(|e| host_err("crypto_aead_encrypt", e))
    }

    fn aead_decrypt(
        &self,
        algorithm: &str,
        key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, HostError> {
        host_crypto::aead_decrypt(algorithm, key, nonce, ciphertext, aad)
            .map_err(|e| host_err("crypto_aead_decrypt", e))
    }

    fn aead_generate_key(&self, algorithm: &str) -> Result<Vec<u8>, HostError> {
        host_crypto::aead_generate_key(algorithm).map_err(|e| host_err("crypto_aead_generate_key", e))
    }

    fn aead_generate_nonce(&self, algorithm: &str) -> Result<Vec<u8>, HostError> {
        host_crypto::aead_generate_nonce(algorithm)
            .map_err(|e| host_err("crypto_aead_generate_nonce", e))
    }

    fn kdf_derive(
        &self,
        algorithm: &str,
        salt: Option<&[u8]>,
        ikm: &[u8],
        info: &[u8],
        length: u32,
    ) -> Result<Vec<u8>, HostError> {
        host_crypto::kdf_derive(algorithm, salt, ikm, info, length)
            .map_err(|e| host_err("crypto_kdf_derive", e))
    }

    fn key_agreement_generate(&self, algorithm: &str) -> Result<CryptoKeypair, HostError> {
        // 宿主返回 private ‖ public 定长字节（x25519 各 32），前 32 私钥、后 32 公钥
        let bytes = host_crypto::keyagreement_generate(algorithm)
            .map_err(|e| host_err("crypto_keyagreement_generate", e))?;
        if bytes.len() != 64 {
            return Err(HostError::custom(
                -1,
                format!("crypto_keyagreement_generate: expected 64 bytes, got {}", bytes.len()),
            ));
        }
        let private = bytes[..32].to_vec();
        let public = bytes[32..].to_vec();
        Ok(CryptoKeypair { private, public })
    }

    fn key_agreement_shared(
        &self,
        algorithm: &str,
        local_private: &[u8],
        peer_public: &[u8],
    ) -> Result<Vec<u8>, HostError> {
        host_crypto::keyagreement_shared(algorithm, local_private, peer_public)
            .map_err(|e| host_err("crypto_keyagreement_shared", e))
    }
}

// ==================== host-platform（ADR 0022 v2）====================

impl HostPlatform for WasmHost {
    fn platform_pick_files(&self) -> Result<Vec<String>, HostError> {
        let v = peer_json(
            "platform_pick_files",
            host_platform::pick_files().map_err(|e| host_err("platform_pick_files", e))?,
        )?;
        serde_json::from_value(v).map_err(|e| {
            HostError::custom(
                -1,
                format!("platform_pick_files: invalid JSON from host: {e}"),
            )
        })
    }

    fn platform_pick_folder(&self) -> Result<String, HostError> {
        host_platform::pick_folder().map_err(|e| host_err("platform_pick_folder", e))
    }

    fn platform_pick_folders(&self) -> Result<Vec<String>, HostError> {
        let v = peer_json(
            "platform_pick_folders",
            host_platform::pick_folders().map_err(|e| host_err("platform_pick_folders", e))?,
        )?;
        serde_json::from_value(v).map_err(|e| {
            HostError::custom(
                -1,
                format!("platform_pick_folders: invalid JSON from host: {e}"),
            )
        })
    }

    fn platform_wsl_distros(&self) -> Result<Vec<String>, HostError> {
        let v = peer_json(
            "platform_wsl_distros",
            host_platform::wsl_distros().map_err(|e| host_err("platform_wsl_distros", e))?,
        )?;
        serde_json::from_value(v).map_err(|e| {
            HostError::custom(
                -1,
                format!("platform_wsl_distros: invalid JSON from host: {e}"),
            )
        })
    }

    fn platform_local_ipv4_addresses(&self) -> Result<Vec<String>, HostError> {
        let v = peer_json(
            "platform_local_ipv4_addresses",
            host_platform::local_ipv4_addresses()
                .map_err(|e| host_err("platform_local_ipv4_addresses", e))?,
        )?;
        serde_json::from_value(v).map_err(|e| {
            HostError::custom(
                -1,
                format!("platform_local_ipv4_addresses: invalid JSON from host: {e}"),
            )
        })
    }

    fn platform_reveal_in_dir(&self, path: &str) -> Result<(), HostError> {
        host_platform::reveal_in_dir(path).map_err(|e| host_err("platform_reveal_in_dir", e))
    }
}
