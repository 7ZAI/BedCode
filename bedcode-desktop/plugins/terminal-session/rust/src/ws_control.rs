//! WS 会话控制域（票 09b/09c：WS 动作词表声明式化的插件侧分派）
//!
//! ## 归属与分工
//!
//! 会话控制动作（list / start / stop / remove / resize）的**词表解释**在宿主
//! 侧随 P1-b 已无业务残留（旧 `services/session_control.rs` 的 match 只是
//! wire → `session_gateway` 的翻译表）；票 09b/09c 把这张翻译表整体迁到本域：
//!
//! - **宿主**：`/ws/event` 旧 `Message::SessionControl` 协议只做传输面三件事——
//!   声明闸门（插件声明 + 激活才转发）、原始动作帧转发（互调 api
//!   `session-ws-control`）、响应动作 JSON 套回 `Message` 信封（H2：线协议
//!   形状类型仍宿主持有）；
//! - **本域**：动作帧「type 是什么、参数怎么用、回包什么形状」的唯一解释方。
//!   宿主不再内联任何动作名语义（票 09c grep 断言）。
//!
//! ## 帧协议（声明端点 `contributes.wsEndpoints: session-control`）
//!
//! 客户端经 `/ws/plugin/com.bedcode.terminal-session/session-control` 直连时，
//! 文本帧即动作 JSON（与旧 `Message::SessionControl.payload.action` 逐字同形）：
//!
//! ```json
//! {"type":"start_session","config_id":"c1"}
//! {"type":"list_sessions"}
//! ```
//!
//! 回包为响应动作 JSON（`session_list` 的 `sessions` 为 snake_case
//! `SessionSummary` 形状，与旧协议逐字一致）；无回包动作（resize）不回帧。
//! 失败回 `{"type":"error","message":"..."}` 文本帧。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostLog, HostWebsocket};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

use crate::actions::{self, RendererSource};
use crate::launch;
use crate::session;

/// 从动作 JSON 取词表名（纯函数，native 单测覆盖；未知/缺 type 显性拒绝）
fn action_type_of(action: &serde_json::Value) -> Result<&str, String> {
    match action.get("type").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => Ok(t),
        _ => Err("session control action missing 'type'".to_string()),
    }
}

/// 动作域分派：动作 JSON → 响应动作 JSON（`null` = 无回包动作，如 resize）
///
/// `source_device`：触发端设备名（移动端 JWT claims；桌面本地为空串）——
/// 透传进创建 / 移除编排（正统端初始归属 / 广播排除语义，与旧宿主路径一致）。
///
/// 响应形状对齐旧宿主 `handle_control` 的 wire：`session_list` 的
/// `sessions` 为 snake_case `SessionSummary`（[`session::summaries_json`]）；
/// start / stop / remove 回显请求载荷（移动端 `Message::SessionControl`
/// 响应反序列化需要变体与字段逐字一致）。
pub fn handle_action(
    action: &serde_json::Value,
    source_device: Option<&str>,
) -> Result<serde_json::Value, String> {
    match action_type_of(action)? {
        "list_sessions" => {
            let summaries = session::summaries_json()?;
            Ok(serde_json::json!({
                "type": "session_list",
                "sessions": summaries.get("sessions").cloned().unwrap_or_else(|| serde_json::json!([])),
            }))
        }
        "start_session" => {
            let config_id = action
                .get("config_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "start_session: missing config_id".to_string())?;
            // 与旧宿主 `handle_control` 的 StartSession 分支同参：创建即启动、
            // 无初始尺寸（None → 配置默认值）、启动端设备名透传
            let draft = serde_json::json!({
                "configId": config_id,
                "start": true,
                "sourceDevice": source_device,
            });
            let created = launch::create_via_host(&draft)?;
            // 响应动作内额外带 `session_id`（新会话 id）：宿主转发层把它填进
            // `Message::SessionControl` 信封的 session_id（旧宿主路径该字段 = 新
            // 会话 id，移动端据此拿到新会话）；动作变体反序列化剥除该字段，
            // 客户端收到的动作形状与旧 wire 逐字一致（增量演进，老端忽略未知字段）
            let session_id = created
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(serde_json::json!({ "type": "start_session", "config_id": config_id, "session_id": session_id }))
        }
        "stop_session" => {
            let session_id = action
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "stop_session: missing session_id".to_string())?;
            let draft = serde_json::json!({ "sessionId": session_id });
            actions::close_via_host(&draft)?;
            Ok(serde_json::json!({ "type": "stop_session", "session_id": session_id }))
        }
        "remove_session" => {
            let session_id = action
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "remove_session: missing session_id".to_string())?;
            let draft = serde_json::json!({
                "sessionId": session_id,
                "sourceDevice": source_device,
            });
            actions::remove_via_host(&draft)?;
            Ok(serde_json::json!({ "type": "remove_session", "session_id": session_id }))
        }
        "resize_session" => {
            let session_id = action
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "resize_session: missing session_id".to_string())?;
            let cols = action
                .get("cols")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .filter(|v| *v > 0)
                .ok_or_else(|| "resize_session: cols must be > 0".to_string())?;
            let rows = action
                .get("rows")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .filter(|v| *v > 0)
                .ok_or_else(|| "resize_session: rows must be > 0".to_string())?;
            let force = action
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // 请求方身份：携带设备名 → 移动端（旧宿主按 device_name 判），
            // 否则桌面端（旧宿主 warn + 按 Desktop 处理）
            let requester = match source_device {
                Some(name) if !name.is_empty() => RendererSource::Mobile {
                    device_name: name.to_string(),
                },
                _ => RendererSource::Desktop,
            };
            let draft = serde_json::json!({
                "sessionId": session_id,
                "cols": cols,
                "rows": rows,
                "requester": requester,
                "force": force,
            });
            // 裁决结果（needsConfirmation / applied）旧宿主只记日志不写回，
            // 此处同样不构造回包动作 → null（host 收 null 走 ack-if-expected）
            actions::resize_via_host(&draft)?;
            Ok(serde_json::Value::Null)
        }
        other => Err(format!("unknown session control action: {other}")),
    }
}

/// 命令面入口（`session.ws.control`，供宿主闭环测试直调，不经总线）
///
/// 入参 `{action: <动作 JSON>, sourceDevice?}`；回执同 [`handle_action`]
/// （null = 无回包动作）。
pub fn handle_command(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let args = bedcode_plugin_api::CommandArgs::new(args.clone());
    let action = args
        .value_owned("action")
        .ok_or_else(|| "session.ws.control: missing action".to_string())?;
    let source_device = args
        .value_owned("sourceDevice")
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|s| !s.is_empty());
    handle_action(&action, source_device.as_deref())
}

/// 从连接上下文 JSON 提取发起者设备名（websocket 业务下沉票 03：直连端点的
/// 「请求关联」——动作帧的发起者身份由插件自 `connection-context` 解析，
/// 宿主不再透传 claims 设备名）
///
/// 只取 `authContext.deviceName`（已脱敏）；`auth: none` 连接无 authContext →
/// `None`（桌面本地）；上下文查询失败 / 非法 JSON → `None`（不伪造身份）。
fn source_device_from_context(context_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(context_json).ok()?;
    value["authContext"]["deviceName"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// events-ws 服务端域回调（声明端点 `session-control` 的入站帧）
///
/// 文本帧 = 动作 JSON（旧 `Message::SessionControl.payload.action` 同形）；
/// 处理后经 `host-websocket.send-text-to-client` 回包（响应动作 JSON；
/// 无回包动作不回帧）。二进制帧非本端点协议 → 忽略 + debug 留痕。
///
/// 发起者身份（票 03）：经 `connection-context` 自取已脱敏 `deviceName`，
/// 作为 `source_device` 透传进创建/移除编排（正统端初始归属/广播排除语义），
/// 宿主不再解释请求关联。
#[cfg(target_arch = "wasm32")]
pub fn on_client_message(endpoint_id: &str, client_id: &str, kind: &str, payload: &[u8]) -> anyhow::Result<()> {
    if kind != "text" {
        WasmHost.log_debug(&format!(
            "session-control: ignoring {kind} frame from {client_id} (protocol is text JSON actions)"
        ));
        return Ok(());
    }
    let text = String::from_utf8_lossy(payload);
    let source_device = WasmHost
        .ws_connection_context(endpoint_id, client_id)
        .ok()
        .and_then(|ctx| source_device_from_context(&ctx));
    let reply = match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(action) => match handle_action(&action, source_device.as_deref()) {
            Ok(reply) if reply.is_null() => None, // 无回包动作（resize）
            Ok(reply) => Some(reply),
            Err(e) => Some(serde_json::json!({ "type": "error", "message": e })),
        },
        Err(e) => Some(serde_json::json!({
            "type": "error",
            "message": format!("invalid session control action frame: {e}"),
        })),
    };
    if let Some(reply) = reply {
        WasmHost
            .ws_send_text_to_client(endpoint_id, client_id, &reply.to_string())
            .map_err(|e| anyhow::anyhow!("session-control reply failed: {}", e.message))?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_message(_endpoint_id: &str, _client_id: &str, _kind: &str, _payload: &[u8]) -> anyhow::Result<()> {
    anyhow::bail!("ws session control unavailable outside wasm runtime")
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 词表分派（纯函数契约） ====================

    #[test]
    fn action_type_extracts_known_types() {
        assert_eq!(
            action_type_of(&serde_json::json!({ "type": "start_session" })).unwrap(),
            "start_session"
        );
        assert_eq!(
            action_type_of(&serde_json::json!({ "type": "list_sessions" })).unwrap(),
            "list_sessions"
        );
    }

    #[test]
    fn action_type_rejects_missing_or_empty() {
        assert!(action_type_of(&serde_json::json!({})).is_err());
        assert!(action_type_of(&serde_json::json!({ "type": "" })).is_err());
        assert!(action_type_of(&serde_json::json!({ "type": 42 })).is_err());
        assert!(action_type_of(&serde_json::json!("start_session")).is_err());
    }

    /// 未知动作名 fail-visible：宿主不解释的词表 → 显性拒绝（不静默吞）
    #[test]
    fn handle_action_rejects_unknown_type_before_host_call() {
        let err = handle_action(
            &serde_json::json!({ "type": "launch_missiles" }),
            None,
        )
        .expect_err("unknown action must fail");
        assert!(err.contains("unknown session control action"), "got: {err}");
    }

    /// 已知动作缺必填参数 → 显性报错（不落半程动作）
    #[test]
    fn handle_action_requires_action_params() {
        for action in [
            serde_json::json!({ "type": "start_session" }),
            serde_json::json!({ "type": "start_session", "config_id": "" }),
            serde_json::json!({ "type": "stop_session" }),
            serde_json::json!({ "type": "remove_session", "session_id": "" }),
            serde_json::json!({ "type": "resize_session", "session_id": "s1", "cols": 0, "rows": 40 }),
        ] {
            assert!(handle_action(&action, None).is_err(), "must reject: {action}");
        }
    }

    /// 命令面入口：缺 action 参数显性报错
    #[test]
    fn handle_command_requires_action_arg() {
        let args = serde_json::json!({});
        assert!(handle_command(&args).is_err());
    }

    // ==================== 票 03：连接上下文 → 发起者身份（请求关联） ====================

    /// 已认证连接：authContext.deviceName → 发起者设备名
    #[test]
    fn source_device_from_authenticated_context() {
        let ctx = r#"{
            "clientId": "127.0.0.1:1",
            "authenticated": true,
            "authContext": {"subject": "dev-1", "deviceName": "Phone", "fingerprint": "fp-1"}
        }"#;
        assert_eq!(source_device_from_context(ctx), Some("Phone".to_string()));
    }

    /// auth:none 连接：无 authContext → None（桌面本地，不伪造身份）
    #[test]
    fn source_device_omitted_for_unauthenticated() {
        let ctx = r#"{"clientId": "127.0.0.1:1", "authenticated": false}"#;
        assert_eq!(source_device_from_context(ctx), None);
    }

    /// 上下文异常（非法 JSON / 缺 authContext / deviceName 空串）→ None
    #[test]
    fn source_device_fails_open_to_none_on_bad_context() {
        assert_eq!(source_device_from_context("not json"), None);
        assert_eq!(source_device_from_context(r#"{"clientId":"c"}"#), None);
        assert_eq!(
            source_device_from_context(r#"{"authContext":{"deviceName":""}}"#),
            None,
            "空设备名不算发起者"
        );
    }

    /// 票 03 结构锁：直连端点的入站帧处理必须经 `connection-context` 自取发起者
    /// 身份（宿主不再透传 claims 设备名）——`on_client_message` 实现段不得缺少
    /// `ws_connection_context(` 调用（改动即红：请求关联回归宿主 = 红线）。
    #[test]
    fn endpoint_message_handler_resolves_identity_via_connection_context() {
        let root = env!("CARGO_MANIFEST_DIR");
        let src = std::fs::read_to_string(format!("{root}/src/ws_control.rs")).expect("read ws_control.rs");
        let handler = src.split("pub fn on_client_message").nth(1).expect("on_client_message 段");
        let calls = handler
            .lines()
            .filter(|l| l.contains("ws_connection_context(") && !l.trim_start().starts_with("//"))
            .count();
        assert_eq!(calls, 1, "on_client_message 必须调用一次 connection-context 解析发起者");
    }
}
