//! 会话控制域（票 13 自宿主 `session::http::SessionHttpClient` + `commands/session.rs`
//! 迁入）：list / start / stop / remove / input(HTTP) 经**插件自有 HTTP 面**
//! （`host-http.fetch`）直连桌面 `/api/sessions*`。
//!
//! ## JWT 注入（C4）
//!
//! 请求 JSON 恒带 `jwtAuth: true`——宿主代注 `Authorization: Bearer <global
//! token>`（token 不落插件，对齐票 12 `host-websocket.jwt-auth` 先例）；
//! 「认证链路只走既有 auth 模块」，本域不接触凭据材料。
//!
//! ## 与终端订阅域（`link.rs`）输入通道的分工
//!
//! - `link.rs::send_input`（WS 帧）：终端页订阅链路的输入，依赖 subscribe 连接
//!   （含 special-key 翻译与帧序护栏）；
//! - 本域 `send-http-input`（HTTP）：绕过订阅的直发通道（TUI 滚轮序列 /
//!   预设任务下发），与订阅状态无关。
//!   两条通道 wire 形状各自对齐退役前实现，互不替代。
//!
//! ## 失败语义（与退役前前端 `useHttpApi` 逐项对齐，前端调用点零改动）
//!
//! - 2xx + 合法 JSON 信封 → 透传 `{code, message, data?}`（业务码非 0 由前端按 code 分支）
//! - 2xx + 非法 JSON → `{code: -1, message}`
//! - 非 2xx → `{code: status, message}`（原前端 `code: resp.status` 语义）
//! - 网络故障 / 未连接 / 权限拒绝 / egress 拒绝 → 命令 Err（前端封装 catch → `{code:-1}`）

use bedcode_plugin_api_mobile::host::HostHttp;
use bedcode_plugin_api_mobile::wasm_host::WasmHost;

use crate::read_primary_target;

/// 会话列表 / 启动端点（与退役前 `session/http.rs` 逐字一致；桌面 wire 契约零变化）
const PATH_SESSIONS: &str = "/api/sessions";
const PATH_SESSIONS_START: &str = "/api/sessions/start";

fn host() -> WasmHost {
    WasmHost
}

// ==================== 命令实现 ====================

/// 会话列表（`GET /api/sessions`）
pub(crate) fn list_sessions() -> anyhow::Result<serde_json::Value> {
    http_call("GET", PATH_SESSIONS, None)
}

/// 启动会话（`POST /api/sessions/start`，body `{configId, cols?, rows?}`）
pub(crate) fn start_session(args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let config_id = require_str(args, "configId")?;
    let cols = args.get("cols").and_then(|v| v.as_u64());
    let rows = args.get("rows").and_then(|v| v.as_u64());
    http_call(
        "POST",
        PATH_SESSIONS_START,
        Some(build_start_body(&config_id, cols, rows)),
    )
}

/// 停止会话（`POST /api/sessions/{id}/stop`）
pub(crate) fn stop_session(args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let session_id = require_str(args, "sessionId")?;
    http_call("POST", &session_path(&session_id, "stop"), None)
}

/// 删除会话（`DELETE /api/sessions/{id}/remove`）
pub(crate) fn remove_session(args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let session_id = require_str(args, "sessionId")?;
    http_call("DELETE", &session_path(&session_id, "remove"), None)
}

/// 写入终端输入（`POST /api/sessions/{id}/input`，body `{data, specialKey?}`）
///
/// 与 `link.rs` 的 WS 帧 `send-input` 是两条通道（见模块头注）；specialKey 由
/// 桌面端翻译，本端不解释。
pub(crate) fn send_http_input(args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let session_id = require_str(args, "sessionId")?;
    let data = args.get("data").and_then(|v| v.as_str()).unwrap_or("");
    let special_key = args.get("specialKey").and_then(|v| v.as_str());
    http_call(
        "POST",
        &session_path(&session_id, "input"),
        Some(build_input_body(data, special_key)),
    )
}

// ==================== HTTP 调用 ====================

/// 经宿主 `host-http` 打一次桌面会话控制请求，返回前端消费形状
/// （`{code, message, data?}`）；网络层失败（未连接 / 权限 / egress / 传输）
/// 一律 Err 上抛。
fn http_call(method: &str, path: &str, body: Option<serde_json::Value>) -> anyhow::Result<serde_json::Value> {
    let target = read_primary_target()?; // 未配置目标 → 显性错误（fail-visible）
    let url = format!("http://{}:{}{}", target.address, target.port, path);
    let request = build_request(method, &url, body);
    let resp = host()
        .http_fetch(&request)
        .map_err(|e| anyhow::anyhow!("session http fetch failed ({method} {path}): {e}"))?;
    let Some(resp) = resp else {
        return Err(anyhow::anyhow!(
            "session http fetch returned empty response ({method} {path})"
        ));
    };
    Ok(classify_http_response(&resp))
}

// ==================== 纯函数（native 单测锚点） ====================

/// `host-http.fetch` 请求 JSON：headers 固定 JSON；`jwtAuth: true` = 宿主代注
/// Bearer（token 不落插件，C4）；body 以字符串承载（宿主契约）。
pub(crate) fn build_request(method: &str, url: &str, body: Option<serde_json::Value>) -> serde_json::Value {
    let mut request = serde_json::json!({
        "method": method,
        "url": url,
        "headers": { "Content-Type": "application/json" },
        "jwtAuth": true,
    });
    if let Some(b) = body {
        request["body"] = serde_json::Value::String(b.to_string());
    }
    request
}

/// 会话子路径 `/api/sessions/{id}/{action}`（stop / remove / input 共用；
/// 与退役前前端模板拼接逐字一致，id 不做 URL 编码——桌面端 id 格式受控）
pub(crate) fn session_path(session_id: &str, action: &str) -> String {
    format!("{PATH_SESSIONS}/{session_id}/{action}")
}

/// 启动会话 body：`configId` 必填；`cols/rows` 仅 Some 时出现
/// （与退役前 `JSON.stringify({configId, cols: size?.cols, rows: size?.rows})`
/// 的缺省省略形态一致——桌面端「两者齐备且 >0 才作 PTY 初始尺寸」）
pub(crate) fn build_start_body(config_id: &str, cols: Option<u64>, rows: Option<u64>) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("configId".to_string(), serde_json::Value::String(config_id.to_string()));
    if let Some(cols) = cols {
        body.insert("cols".to_string(), serde_json::json!(cols));
    }
    if let Some(rows) = rows {
        body.insert("rows".to_string(), serde_json::json!(rows));
    }
    serde_json::Value::Object(body)
}

/// 输入 body：`{data, specialKey}`（specialKey 无值 → null，与退役前
/// `specialKey: specialKey || null` 一致；桌面端翻译特殊键）
pub(crate) fn build_input_body(data: &str, special_key: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "data": data,
        "specialKey": special_key,
    })
}

/// HTTP 响应（宿主 `{status, body, headers}`）→ 前端消费形状
/// （语义逐项对齐退役前 `useHttpApi.request`，见模块头注）
pub(crate) fn classify_http_response(resp: &serde_json::Value) -> serde_json::Value {
    let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
    if !(200..300).contains(&status) {
        return serde_json::json!({ "code": status, "message": format!("HTTP {status}") });
    }
    let body = resp.get("body").and_then(|v| v.as_str()).unwrap_or("");
    match serde_json::from_str::<serde_json::Value>(body) {
        // 信封必须是对象（桌面契约）；数组/标量按畸形处理
        Ok(v) if v.is_object() => v,
        _ => serde_json::json!({
            "code": -1,
            "message": "invalid response JSON from desktop",
        }),
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing {key}"))
}

// ==================== 单元测试（native；纯函数锚点，不触 host） ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 请求构造 ----------

    #[test]
    fn build_request_always_carries_jwt_auth() {
        let req = build_request("GET", "http://192.168.1.5:8765/api/sessions", None);
        assert_eq!(req["method"], "GET");
        assert_eq!(req["url"], "http://192.168.1.5:8765/api/sessions");
        assert_eq!(req["jwtAuth"], true, "JWT 注入恒开（宿主代注 Bearer）");
        assert_eq!(req["headers"]["Content-Type"], "application/json");
        assert!(req.get("body").is_none(), "无 body 时不得出现 body 字段");
    }

    #[test]
    fn build_request_serializes_body_as_string() {
        let body = serde_json::json!({ "configId": "cfg-1" });
        let req = build_request("POST", "http://x/api/sessions/start", Some(body.clone()));
        // 宿主契约：body 为字符串承载（不是嵌套对象）
        assert_eq!(req["body"], serde_json::Value::String(body.to_string()));
    }

    #[test]
    fn session_path_shapes() {
        assert_eq!(session_path("s-1", "stop"), "/api/sessions/s-1/stop");
        assert_eq!(session_path("s-1", "remove"), "/api/sessions/s-1/remove");
        assert_eq!(session_path("s-1", "input"), "/api/sessions/s-1/input");
    }

    #[test]
    fn start_body_omits_absent_size() {
        // 与退役前 JSON.stringify 的缺省省略形态一致
        let body = build_start_body("cfg-1", None, None);
        assert_eq!(body, serde_json::json!({ "configId": "cfg-1" }));
        assert!(body.get("cols").is_none());
        assert!(body.get("rows").is_none());

        let sized = build_start_body("cfg-1", Some(120), Some(30));
        assert_eq!(sized, serde_json::json!({ "configId": "cfg-1", "cols": 120, "rows": 30 }));
    }

    #[test]
    fn input_body_carries_null_special_key_when_absent() {
        assert_eq!(
            build_input_body("ls -la", None),
            serde_json::json!({ "data": "ls -la", "specialKey": null })
        );
        assert_eq!(
            build_input_body("", Some("ctrl+c")),
            serde_json::json!({ "data": "", "specialKey": "ctrl+c" })
        );
    }

    // ---------- 响应分类（与退役前 useHttpApi 语义逐项对齐） ----------

    #[test]
    fn classify_passes_through_envelope() {
        let resp = serde_json::json!({
            "status": 200,
            "body": r#"{"code":0,"message":"ok","data":{"sessions":[{"id":"s1","status":"running"}]}}"#,
        });
        let out = classify_http_response(&resp);
        assert_eq!(out["code"], 0);
        assert_eq!(out["message"], "ok");
        assert_eq!(out["data"]["sessions"][0]["id"], "s1");
    }

    #[test]
    fn classify_passes_through_business_error_envelope() {
        // 桌面错误口径：HTTP 200 + `{code:1002, message}`（业务码由前端分支）
        let resp = serde_json::json!({ "status": 200, "body": r#"{"code":1002,"message":"session not found"}"# });
        let out = classify_http_response(&resp);
        assert_eq!(out["code"], 1002);
        assert_eq!(out["message"], "session not found");
        assert!(out.get("data").is_none(), "无 data 的 ok 信封不伪造 data");
    }

    #[test]
    fn classify_non_2xx_maps_to_status_code() {
        for status in [404u64, 500] {
            let resp = serde_json::json!({ "status": status, "body": "boom" });
            let out = classify_http_response(&resp);
            assert_eq!(out["code"], status, "非 2xx 应透出 HTTP 状态码（前端按 code 分支）");
            assert!(out["message"].as_str().unwrap().contains(&status.to_string()));
        }
    }

    #[test]
    fn classify_malformed_body_is_minus_one() {
        // 2xx 但 body 非 JSON 对象 → code -1（与前端 JSON.parse 失败 catch 语义一致）
        for body in ["not-json", "[]", "\"str\"", ""] {
            let resp = serde_json::json!({ "status": 200, "body": body });
            let out = classify_http_response(&resp);
            assert_eq!(out["code"], -1, "body={body} 应映射 code -1");
            assert!(out["message"].as_str().unwrap().contains("invalid response JSON"));
        }
    }

    #[test]
    fn require_str_rejects_missing_and_empty() {
        let args = serde_json::json!({ "sessionId": "" });
        assert!(require_str(&args, "sessionId").is_err(), "空串同样拒绝");
        assert!(require_str(&args, "configId").is_err());
        assert_eq!(
            require_str(&serde_json::json!({ "configId": "cfg-1" }), "configId").unwrap(),
            "cfg-1"
        );
    }
}
