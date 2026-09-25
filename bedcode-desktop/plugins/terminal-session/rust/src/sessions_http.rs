//! sessions REST HTTP 域（ABI v29 动态路由：`/api/sessions*` 七条，票 11 下沉收尾）
//!
//! 移动端会话 REST 面（list / start / stop / resize / input / history / remove）
//! 从宿主 `session_controller.rs` 整体下沉到本插件：宿主不再注册这些业务路由，
//! 插件在 activate 期注册模板别名（`/api/sessions` / `/api/sessions/{id}/…`），
//! 网关按注册表转发、模板捕获 `{id}` 经 `params` 字段注入。
//!
//! **响应形状逐字节对齐旧宿主控制器**（移动端契约）：成功 `{code:0, message:"ok",
//! data:…}` + HTTP 200；失败 **HTTP 200 + `{code:1002, message}`**（旧控制器的错误
//! 口径——**不是** `http_response::error(4xx, …)`，那会把 HTTP 状态码一起改掉）。
//! 时间戳与状态字面量沿用旧控制器口径（RFC3339 / SessionStatus wire 串）。

use crate::{SessionApi, SessionPlugin};
use bedcode_plugin_api::host::{HostEvents, HostLog};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::WasmHost;
use serde_json::Value;

/// sessions REST 内部端点段（`_http_endpoint` 分派键；host 模板在 `http_routes`）
pub const SESSIONS_HTTP_PATHS: &[&str] = &[
    "sessions",
    "sessions/start",
    "sessions/stop",
    "sessions/resize",
    "sessions/input",
    "sessions/history",
    "sessions/remove",
];

/// 旧控制器的错误信封：HTTP 200 + `{code:1002, message}`（业务码而非 HTTP 码）
fn sessions_error(message: &str) -> Value {
    serde_json::json!({ "status": 200, "body": { "code": 1002, "message": message } })
}

/// 设备名（JWT claims 派生，宿主转发 `device.deviceName`；无 → 桌面/缺省源）
fn device_name(device: &Value) -> Option<String> {
    device
        .get("deviceName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 模板捕获的 session id（`/api/sessions/{id}/…` 的 `{id}` 段；缺失显性报错）
fn session_id_from_params(params: &Value) -> Result<String, String> {
    params
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "sessionId required (template capture 'id' missing)".to_string())
}

/// sessions REST 分派入口（与业务域 / 任务域并列，path 全等匹配）
pub fn handle_sessions_http(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &Value,
    query: &Value,
    params: &Value,
    device: &Value,
) -> Value {
    match path {
        "sessions" => list_sessions(host, method),
        "sessions/start" => start_session(host, method, body, device),
        "sessions/stop" => stop_session(host, method, params, device),
        "sessions/resize" => resize_session(host, method, params, body, device),
        "sessions/input" => input_session(host, method, params, body),
        "sessions/history" => history_session(host, method, params, query),
        "sessions/remove" => remove_session(host, method, params, device),
        _ => http_response::error(404, &format!("Unknown session endpoint: {path}")),
    }
}

/// 会话列表（GET /api/sessions）→ `{sessions: SessionItem[]}`（旧控制器逐字段形状）
fn list_sessions(host: &WasmHost, method: &str) -> Value {
    if method != "GET" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let views = match SessionPlugin::session_list(serde_json::json!({})) {
        Ok(v) => v
            .get("sessions")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(e) => {
            host.log_warn(&format!("sessions list http: {e}"));
            return sessions_error(&e);
        }
    };
    let items: Vec<Value> = views
        .into_iter()
        .map(|view| {
            let mut item = serde_json::Map::new();
            let take = |key: &str| view.get(key).cloned().unwrap_or(Value::Null);
            item.insert("id".to_string(), take("id"));
            item.insert("name".to_string(), take("name"));
            item.insert("status".to_string(), take("status"));
            item.insert("createdAt".to_string(), take("createdAt"));
            item.insert("startedAt".to_string(), take("startedAt"));
            item.insert("sessionType".to_string(), Value::String("pty".to_string()));
            item.insert("configId".to_string(), take("configId"));
            // 任务字段（槽有值才出现——skip_serializing_if 同款缺席形态）
            for key in ["taskStatus", "taskReason"] {
                if view.get(key).is_some() {
                    item.insert(key.to_string(), take(key));
                }
            }
            Value::Object(item)
        })
        .collect();
    http_response::ok_with_data(serde_json::json!({ "sessions": items }))
}

/// 启动会话（POST /api/sessions/start）→ `{sessionId, status:"running"}`
///
/// 与旧控制器同判据：cols/rows 两者齐备且 >0 才作为 PTY 初始尺寸；源设备名
/// 参与正统端初始归属与前端刷新事件。
fn start_session(host: &WasmHost, method: &str, body: &Value, device: &Value) -> Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let config_id = body
        .get("configId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "configId required".to_string());
    let config_id = match config_id {
        Ok(c) => c,
        Err(e) => return sessions_error(&e),
    };
    let num = |k: &str| body.get(k).and_then(|v| v.as_u64()).map(|v| v as u16);
    let initial_size = match (num("cols"), num("rows")) {
        (Some(cols), Some(rows)) if cols > 0 && rows > 0 => (Some(cols), Some(rows)),
        _ => (None, None),
    };
    let source_device = device_name(device);
    let draft = serde_json::json!({
        "configId": config_id,
        "cols": initial_size.0,
        "rows": initial_size.1,
        "start": true,
        "sourceDevice": source_device,
    });
    match SessionPlugin::session_create(draft) {
        Ok(v) => {
            let session_id = v
                .get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();
            if session_id.is_empty() {
                return sessions_error("session-create reply missing sessionId");
            }
            // 前端刷新通知（旧控制器 emit `sessions-refresh` 同语义；emit 为
            // fire-and-forget，失败只落宿主日志）
            host.emit_event(
                "sessions-refresh",
                &serde_json::json!({
                    "refreshType": "sessions",
                    "source": source_device.clone().unwrap_or_else(|| "mobile".to_string()),
                }),
            );
            http_response::ok_with_data(serde_json::json!({
                "sessionId": session_id,
                "status": "running",
            }))
        }
        Err(e) => {
            host.log_warn(&format!("sessions start http: {e}"));
            sessions_error(&e)
        }
    }
}

/// 停止会话（POST /api/sessions/{id}/stop）→ `{code:0, message:"ok"}` + 前端刷新
fn stop_session(host: &WasmHost, method: &str, params: &Value, device: &Value) -> Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = match session_id_from_params(params) {
        Ok(id) => id,
        Err(e) => return sessions_error(&e),
    };
    let source_device = device_name(device);
    match SessionPlugin::session_close(serde_json::json!({ "sessionId": session_id })) {
        Ok(_) => {
            host.emit_event(
                "sessions-refresh",
                &serde_json::json!({
                    "refreshType": "sessions",
                    "source": source_device.clone().unwrap_or_else(|| "mobile".to_string()),
                }),
            );
            http_response::ok()
        }
        Err(e) => {
            host.log_warn(&format!("sessions stop http: {e}"));
            sessions_error(&e)
        }
    }
}

/// 移除会话（DELETE /api/sessions/{id}/remove）→ `{code:0, message:"ok"}` + 前端刷新
fn remove_session(host: &WasmHost, method: &str, params: &Value, device: &Value) -> Value {
    if method != "DELETE" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = match session_id_from_params(params) {
        Ok(id) => id,
        Err(e) => return sessions_error(&e),
    };
    let source_device = device_name(device);
    match SessionPlugin::session_remove(serde_json::json!({
        "sessionId": session_id,
        "sourceDevice": source_device,
    })) {
        Ok(_) => {
            host.emit_event(
                "sessions-refresh",
                &serde_json::json!({
                    "refreshType": "sessions",
                    "source": source_device.clone().unwrap_or_else(|| "mobile".to_string()),
                }),
            );
            http_response::ok()
        }
        Err(e) => {
            host.log_warn(&format!("sessions remove http: {e}"));
            sessions_error(&e)
        }
    }
}

/// 尺寸调整（POST /api/sessions/{id}/resize）→ ResizeOutcome（wire 同旧控制器）
///
/// 来源身份：JWT claims 的 deviceName（移动端）→ Mobile；无 → Desktop（仍受
/// NeedsConfirmation 门控，不会静默覆盖）。
fn resize_session(
    host: &WasmHost,
    method: &str,
    params: &Value,
    body: &Value,
    device: &Value,
) -> Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = match session_id_from_params(params) {
        Ok(id) => id,
        Err(e) => return sessions_error(&e),
    };
    let cols = body.get("cols").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    let rows = body.get("rows").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    let force = body.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    let requester = match device_name(device) {
        Some(name) => serde_json::json!({ "kind": "mobile", "deviceName": name }),
        None => serde_json::json!({ "kind": "desktop" }),
    };
    match SessionPlugin::session_resize(serde_json::json!({
        "sessionId": session_id,
        "cols": cols,
        "rows": rows,
        "requester": requester,
        "force": force,
    })) {
        Ok(outcome) => http_response::ok_with_data(outcome),
        Err(e) => {
            host.log_warn(&format!("sessions resize http: {e}"));
            sessions_error(&e)
        }
    }
}

/// 写入输入（POST /api/sessions/{id}/input）：普通数据 + 可选特殊键（组合串）
fn input_session(host: &WasmHost, method: &str, params: &Value, body: &Value) -> Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = match session_id_from_params(params) {
        Ok(id) => id,
        Err(e) => return sessions_error(&e),
    };
    let data = body.get("data").and_then(|v| v.as_str()).unwrap_or("");
    let special_key = body
        .get("specialKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    // 与旧控制器同序：普通输入先写，特殊键再写（两者都可缺席），任一失败即报错
    if !data.is_empty() {
        if let Err(e) = SessionPlugin::session_input(serde_json::json!({
            "sessionId": session_id,
            "data": data,
        })) {
            host.log_warn(&format!("sessions input http: {e}"));
            return sessions_error(&e);
        }
    }
    if let Some(key) = special_key {
        if let Err(e) = SessionPlugin::session_input(serde_json::json!({
            "sessionId": session_id,
            "data": "",
            "specialKey": key,
        })) {
            host.log_warn(&format!("sessions special key http: {e}"));
            return sessions_error(&e);
        }
    }
    http_response::ok()
}

/// 历史快照（GET /api/sessions/{id}/history）→ 字节三件套 + Base64 data（同旧控制器）
fn history_session(host: &WasmHost, method: &str, params: &Value, query: &Value) -> Value {
    if method != "GET" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = match session_id_from_params(params) {
        Ok(id) => id,
        Err(e) => return sessions_error(&e),
    };
    let from = query.get("from").and_then(|v| v.as_u64()).unwrap_or(0);
    let draft = serde_json::json!({ "sessionId": session_id, "from": from });
    match SessionPlugin::session_history(draft) {
        Ok(v) => {
            let data: Vec<u8> = v
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| b.as_u64().map(|b| b as u8))
                        .collect()
                })
                .unwrap_or_default();
            let min_offset = v.get("minOffset").and_then(|n| n.as_u64()).unwrap_or(0);
            let snapshot_offset = v
                .get("snapshotOffset")
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let history_bytes = v.get("historyBytes").and_then(|n| n.as_u64()).unwrap_or(0);
            use base64::Engine as _;
            let data_base64 = base64::engine::general_purpose::STANDARD.encode(&data);
            http_response::ok_with_data(serde_json::json!({
                "minOffset": min_offset,
                "snapshotOffset": snapshot_offset,
                "historyBytes": history_bytes,
                "dataBase64": data_base64,
            }))
        }
        Err(e) => {
            host.log_debug(&format!("sessions history http: {e}"));
            sessions_error(&e)
        }
    }
}
