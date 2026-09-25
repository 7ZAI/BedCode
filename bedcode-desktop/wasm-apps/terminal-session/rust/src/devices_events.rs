//! 设备连接事件域（websocket 业务下沉票 07：设备派生与认证记录归插件）
//!
//! ## 归属变化
//!
//! 宿主 `server/ws/conn.rs` 曾在旧通道（`/ws/event` / `/ws/terminal`）认证/断连时
//! emit `device-connected` / `device-disconnected`（Tauri 事件）并代本插件 touch/
//! close 认证记录（`auth_center::notify_connection_*` → 互调 api）。票 07 起设备
//! 事件与认证记录由**本插件自己**经 WS 生命周期事件驱动：
//!
//! - 订阅 `<owner>::ws:client-connect|client-disconnect`（属主私有 topic，仅端点
//!   连接触发；旧通道在票 08 整体退役，届时本域就是唯一设备事件源）；
//! - 事件到达后经 `host-websocket.connection-context` 取**已脱敏**身份
//!   （fingerprint / deviceName / subject），**凭据不出宿主**；
//! - 认证记录 touch/close（本插件私有库 `auth_records` 域，原互调 api 的既有
//!   实现）由插件自驱；宿主不再为端点连接代做。
//!
//! ## 前端事件
//!
//! emit `device:connected` / `device:disconnected`（SDK `EVENT_DEVICE_*`），载荷
//! camelCase：`{ addr, deviceId?, deviceName?, fingerprint? }`——只含已脱敏身份
//! 与连接事实，不泄露 token/密钥。前端（notifications / useDeviceCenter）据此
//! 自建在线去重（离线→在线跃迁才提示），与启动期 `connect-list` 种子化基线同构。
//!
//! ## 失败口径
//!
//! `connection-context` 查询失败 / 缺 fingerprint（auth:none 端点）→ 跳过认证
//! 记录 touch/close 与事件发布（该连接不是配对设备连接，无身份可记）；不伪造。
//! touch/close 失败显性 `log_warn`（认证记录是持久真源，写入失败必须可见）。
//!
//! ## 断开事件的竞态（为什么接入期记忆身份）
//!
//! 宿主 `client-disconnect` 事件**先于**注册表摘除发布（spec §7.1），但 bus 投递是
//! 异步的：插件收到事件时连接可能已被摘除，此时 `connection-context` 会查不到
//! （"client not found"）→ 拿不到指纹 → close/事件丢失。因此**接入期**（连接仍
//! 在册）捕获完整脱敏身份并记忆（`CONNECTED_DEVICES`），断开期直接消费，不再
//! 查询；记忆缺失（插件重启等）才兑底查一次（仍可能失败，按无身份跳过）。

#[cfg(any(target_arch = "wasm32", test))]
use bedcode_plugin_api::constants::{EVENT_DEVICE_CONNECTED, EVENT_DEVICE_DISCONNECTED};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostEvents, HostLog, HostWebsocket};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 接入期捕获的连接身份（client_id → 已脱敏身份）——断开事件消费后即删，
/// 只持公开事实（凭据不出宿主）。插件停用时 [`clear_connected`] 清空。
///
/// OnceLock（与 lib.rs QR_MANAGER 同模式）：`HashMap::new` 非常量，
/// 不能直接作 static 初始化；首次访问经 [`connected_devices`] 初始化一次。
static CONNECTED_DEVICES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, DeviceIdentity>>
> = std::sync::OnceLock::new();

fn connected_devices() -> &'static std::sync::Mutex<std::collections::HashMap<String, DeviceIdentity>> {
    CONNECTED_DEVICES.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// ==================== 载荷构造（纯函数，native 可测） ====================

/// 从连接上下文 JSON 提取已脱敏身份字段（camelCase，与 `connection-context`
/// 形状一致：`{ addr, authenticated, authContext?: { subject, deviceName,
/// fingerprint } }`）。`auth: none` / 上下文异常 → 空字段（不伪造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub addr: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub fingerprint: Option<String>,
}

pub fn identity_from_context(context_json: &str) -> DeviceIdentity {
    let value: serde_json::Value = serde_json::from_str(context_json).unwrap_or_default();
    let auth = value.get("authContext").unwrap_or(&serde_json::Value::Null);
    DeviceIdentity {
        addr: value
            .get("addr")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        device_id: auth.get("subject").and_then(|v| v.as_str()).map(str::to_string),
        device_name: auth.get("deviceName").and_then(|v| v.as_str()).map(str::to_string),
        fingerprint: auth.get("fingerprint").and_then(|v| v.as_str()).map(str::to_string),
    }
}

/// 前端事件载荷（camelCase；只含已脱敏身份与连接事实）
pub fn event_payload(identity: &DeviceIdentity, connected: bool) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "addr": identity.addr,
        "connected": connected,
    });
    if let Some(id) = &identity.device_id {
        payload["deviceId"] = serde_json::json!(id);
    }
    if let Some(name) = &identity.device_name {
        payload["deviceName"] = serde_json::json!(name);
    }
    if let Some(fp) = &identity.fingerprint {
        payload["fingerprint"] = serde_json::json!(fp);
    }
    payload
}

// ==================== wasm：连接事件驱动（touch / close + emit） ====================

/// 解析连接身份（`connection-context`；失败 → `None`，调用方按无身份处理）
#[cfg(target_arch = "wasm32")]
fn resolve_identity(endpoint_id: &str, client_id: &str) -> Option<DeviceIdentity> {
    let ctx = WasmHost
        .ws_connection_context(endpoint_id, client_id)
        .ok()?;
    Some(identity_from_context(&ctx))
}

/// 设备上线（`ws:client-connect` 驱动）：
/// 指纹存在 → 记忆身份 + 认证记录 touch + emit `device:connected`；无指纹 → 跳过
#[cfg(target_arch = "wasm32")]
pub fn on_client_connected(endpoint_id: &str, client_id: &str) {
    let Some(identity) = resolve_identity(endpoint_id, client_id) else {
        WasmHost.log_debug(&format!(
            "device: connect event skipped (connection-context unavailable, client_id={client_id})"
        ));
        return;
    };
    let Some(fp) = identity.fingerprint.clone() else {
        WasmHost.log_debug(&format!(
            "device: connect event skipped (unauthenticated, client_id={client_id})"
        ));
        return;
    };
    // 接入期记忆身份：断开事件到达时连接可能已被宿主摘除（bus 投递异步竞态），
    // 届时 connection-context 查不到——断开期直接消费本表，不再查询
    connected_devices()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(client_id.to_string(), identity.clone());
    // 认证记录 touch（本插件私有库真源；失败显性留痕，不阻断事件发布）
    if let Err(e) = crate::auth_records::touch(&fp) {
        WasmHost.log_warn(&format!("device: auth record touch failed (fingerprint len={}): {e}", fp.len()));
    }
    WasmHost.emit_event(EVENT_DEVICE_CONNECTED, &event_payload(&identity, true));
    WasmHost.log_info(&format!(
        "device connected (client_id={client_id}, fingerprint len={}, device={})",
        fp.len(),
        identity.device_name.as_deref().unwrap_or("")
    ));
}

/// 设备下线（`ws:client-disconnect` 驱动）：
/// 指纹存在 → 认证记录 close + emit `device:disconnected`；无指纹 → 跳过。
/// 身份优先取接入期记忆（免查询）；记忆缺失（插件重启）才兑底查一次。
#[cfg(target_arch = "wasm32")]
pub fn on_client_disconnected(endpoint_id: &str, client_id: &str) {
    let identity = connected_devices()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(client_id)
        .or_else(|| resolve_identity(endpoint_id, client_id));
    let Some(identity) = identity else {
        WasmHost.log_debug(&format!(
            "device: disconnect event skipped (no remembered identity, client_id={client_id})"
        ));
        return;
    };
    let Some(fp) = identity.fingerprint.clone() else {
        WasmHost.log_debug(&format!(
            "device: disconnect event skipped (unauthenticated, client_id={client_id})"
        ));
        return;
    };
    if let Err(e) = crate::auth_records::close_open_connection(&fp) {
        WasmHost.log_warn(&format!(
            "device: auth record close failed (fingerprint len={}): {e}",
            fp.len()
        ));
    }
    WasmHost.emit_event(EVENT_DEVICE_DISCONNECTED, &event_payload(&identity, false));
    WasmHost.log_info(&format!(
        "device disconnected (client_id={client_id}, fingerprint len={}, device={})",
        fp.len(),
        identity.device_name.as_deref().unwrap_or("")
    ));
}

/// 插件停用：清空接入期记忆（连接由宿主按属主回收关闭，断开事件可能不再投递）
pub fn clear_connected() {
    connected_devices()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_connected(_endpoint_id: &str, _client_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_disconnected(_endpoint_id: &str, _client_id: &str) {}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_context_json(fp: &str, name: &str, subject: &str) -> String {
        format!(
            r#"{{"clientId":"127.0.0.1:1","addr":"127.0.0.1:1","authenticated":true,"authContext":{{"subject":"{subject}","deviceName":"{name}","fingerprint":"{fp}"}}}}"#
        )
    }

    /// 已认证连接上下文 → 完整身份（fingerprint / deviceName / subject / addr）
    #[test]
    fn identity_extracted_from_authenticated_context() {
        let identity = identity_from_context(&auth_context_json("fp-1", "Phone", "dev-1"));
        assert_eq!(identity.fingerprint.as_deref(), Some("fp-1"));
        assert_eq!(identity.device_name.as_deref(), Some("Phone"));
        assert_eq!(identity.device_id.as_deref(), Some("dev-1"));
        assert_eq!(identity.addr, "127.0.0.1:1");
    }

    /// auth:none 连接（无 authContext）→ 空身份（不伪造）
    #[test]
    fn identity_empty_for_unauthenticated_context() {
        let identity = identity_from_context(
            r#"{"clientId":"127.0.0.1:2","addr":"127.0.0.1:2","authenticated":false}"#,
        );
        assert_eq!(identity.fingerprint, None);
        assert_eq!(identity.device_name, None);
        assert_eq!(identity.device_id, None);
    }

    /// 非法上下文 → 空身份（fail-open 到无设备，不 panic）
    #[test]
    fn identity_empty_for_invalid_context() {
        let identity = identity_from_context("not json");
        assert_eq!(identity.fingerprint, None);
        assert_eq!(identity.addr, "");
    }

    /// 事件载荷：camelCase + 缺省字段不出现键；凭据字段绝不出现
    #[test]
    fn event_payload_shape_is_camel_case_and_credential_free() {
        let identity = identity_from_context(&auth_context_json("fp-2", "Pad", "dev-2"));
        let payload = event_payload(&identity, true);
        assert_eq!(payload["addr"], "127.0.0.1:1");
        assert_eq!(payload["connected"], true);
        assert_eq!(payload["fingerprint"], "fp-2");
        assert_eq!(payload["deviceName"], "Pad");
        assert_eq!(payload["deviceId"], "dev-2");
        let flat = payload.to_string();
        for forbidden in ["token", "secret", "publicKey", "private", "jwt"] {
            assert!(!flat.to_lowercase().contains(forbidden), "载荷不得泄漏凭据: {forbidden}");
        }
    }

    /// 事件名 = 前端 events.on 的 key（常量锁）
    #[test]
    fn event_names_match_frontend_keys() {
        assert_eq!(EVENT_DEVICE_CONNECTED, "device:connected");
        assert_eq!(EVENT_DEVICE_DISCONNECTED, "device:disconnected");
    }

    /// 接入期记忆：断开事件消费后即删（每连接恰好一次），
    /// 停用清空不残留（插件重启后不会误关旧连接）
    #[test]
    fn connected_identity_is_remembered_then_consumed_once() {
        connected_devices()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        let identity = identity_from_context(&auth_context_json("fp-3", "Phone", "dev-3"));
        connected_devices()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("c-1".to_string(), identity);
        // 断开消费：第一次命中（拿到记忆身份），第二次为 None（不重复 close）
        let taken = connected_devices()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove("c-1");
        assert!(taken.is_some(), "断开事件必须消费接入期记忆的身份");
        assert_eq!(taken.unwrap().fingerprint.as_deref(), Some("fp-3"));
        assert!(
            connected_devices()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove("c-1")
                .is_none(),
            "同连接第二次断开不得再命中"
        );
        // 停用清空：残留记忆不跨生命周期
        connected_devices()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("c-2".to_string(), identity_from_context(&auth_context_json("fp-4", "Pad", "dev-4")));
        clear_connected();
        assert!(
            connected_devices()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty(),
            "插件停用必须清空接入期记忆"
        );
    }

    /// 结构锁：设备事件生产路径**不再依赖宿主设备 DTO / Tauri 事件**——
    /// 本文件实现段不得出现 `DeviceConnectionEvent` / `device-connected` /
    /// `get_connected_devices`；身份只经 `connection-context` 解析。
    #[test]
    fn device_events_do_not_depend_on_host_device_dto() {
        let root = env!("CARGO_MANIFEST_DIR");
        let src = std::fs::read_to_string(format!("{root}/src/devices_events.rs")).expect("read devices_events.rs");
        let implementation = src.split("#[cfg(test)]").next().unwrap_or(&src);
        for (idx, raw) in implementation.lines().enumerate() {
            let line = raw.trim_start();
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            for marker in [
                "DeviceConnectionEvent",
                "DeviceConnectionInfo",
                "device-connected",
                "device-disconnected",
                "get_connected_devices",
                "session_count",
            ] {
                assert!(
                    !line.contains(marker),
                    "{}:{}: 不得出现宿主设备 DTO/事件字眼: {}",
                    "devices_events.rs",
                    idx + 1,
                    line.trim()
                );
            }
        }
        // 身份解析必经 connection-context
        assert!(
            implementation.contains("ws_connection_context"),
            "设备事件身份必须经 connection-context 解析（票 07）"
        );
    }
}
