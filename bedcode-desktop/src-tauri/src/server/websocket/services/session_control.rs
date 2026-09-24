//! Session Control Service（WS 会话控制面：声明式路由转发层）
//!
//! 票 09b/09c：会话控制动作的**词表解释**已整体迁入 `com.bedcode.terminal-session`
//! 插件（manifest `contributes.wsEndpoints` 声明 `session-control`，动作分派实现见
//! 插件 `ws_control` 域）。宿主此层只保留传输面契约（H2：线协议形状类型宿主
//! 持有，业务语义不落宿主）：
//!
//! 1. **声明闸门**：会话控制端点已声明且注册（激活期登记，票 09a）且插件已激活
//!    才转发；未声明 / 未激活 → 显性报错（会话真源已不在宿主，无降级轨）；
//! 2. **转发**：原始动作 JSON → 插件互调 api `session-ws-control`——宿主不解
//!    「`start_session` 是什么业务」，动作名 / 参数 / 回包形状由插件解释；
//! 3. **回包**：插件响应动作 JSON → `Message::SessionControl` 信封（原
//!    message_id；`null` = 无回包动作（resize）→ ack-if-expected）。
//!
//! 旧宿主硬编码词表 switch（`handle_control` 的 `match action { ... }` 逐臂调
//! `session_gateway`）已随本票删除——grep 断言宿主 WS 层无业务词表分发。

use crate::enums::{SessionControlAction, SessionControlPayload};
use crate::server::websocket::message::Message;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::{AppError, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 会话控制端点属主（与插件 manifest `contributes.wsEndpoints` 声明逐字同源）
const SESSION_CONTROL_PLUGIN: &str = "com.bedcode.terminal-session";
/// 会话控制端点路径后缀（插件声明；WS 端点单段约束，与 host-websocket
/// `register-endpoint` 同口径——挂载 `/ws/plugin/<id>/session-control`）
const SESSION_CONTROL_PATH: &str = "session-control";
/// 转发互调 api（插件 `#[api("session-ws-control")]`，实现见插件 `ws_control` 域）
const API_WS_SESSION_CONTROL: &str = "com.bedcode.terminal-session.session-ws-control";

/// 刷新事件类型
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshEvent {
    pub refresh_type: String,
    pub source: String,
}

/// 声明闸门：会话控制端点已声明且注册（激活期登记，票 09a）
///
/// 端点表按完整挂载路径登记；`find_by_mount` 查不到 ⇔ 插件未声明或已停用
/// （deactivate 会 `purge_for_plugin` 回收端点）——两条都是「宿主不解动作」
/// 的边界：未声明 = 宿主对该动作无解释义务，显性拒绝而非静默降级。
pub fn session_control_declared() -> bool {
    let mount = crate::server::websocket::endpoint::mount_path(SESSION_CONTROL_PLUGIN, SESSION_CONTROL_PATH);
    crate::server::websocket::endpoint::find_by_mount(&mount).is_some()
}

/// 转发动作到插件声明路由 → 响应动作 JSON（`None` = 无回包动作）
///
/// `source_device`：触发端设备名（移动端 JWT claims；桌面本地为空串）——
/// 传输面事实（宿主的客户端身份），透传给插件进创建 / 移除编排
/// （正统端初始归属 / 广播排除语义，与旧宿主路径一致）。
async fn relay_action(
    host_ctx: &Arc<WasmHostContext>,
    action: &SessionControlAction,
    source_device: Option<&str>,
) -> Result<Option<serde_json::Value>> {
    // 声明闸门 + 激活闸门（双闸：端点存在 ⇔ 已声明；session_active ⇔ 互调面登记）
    if !session_control_declared() {
        tracing::warn!(
            plugin_id = %SESSION_CONTROL_PLUGIN,
            declared = %SESSION_CONTROL_PATH,
            "session control relay refused: endpoint not declared/active"
        );
        return Err(AppError::Plugin(format!(
            "session control endpoint not declared/active: {SESSION_CONTROL_PLUGIN}/{SESSION_CONTROL_PATH}"
        )));
    }
    if !session_active(host_ctx) {
        tracing::warn!(api = %API_WS_SESSION_CONTROL, "session control relay refused: session plugin not active");
        return Err(AppError::Plugin(
            "session plugin not active: session control requires com.bedcode.terminal-session".to_string(),
        ));
    }

    let action_json = serde_json::to_value(action)
        .map_err(|e| AppError::Plugin(format!("session control action serialize failed: {e}")))?;
    let params = serde_json::json!({ "action": action_json, "sourceDevice": source_device });
    let reply = call_api(host_ctx, API_WS_SESSION_CONTROL, params).map_err(|e| {
        tracing::error!(api = %API_WS_SESSION_CONTROL, error = %e, "session control relay failed via plugin");
        AppError::Plugin(format!("session control failed (plugin error): {e}"))
    })?;
    if reply.is_null() {
        return Ok(None);
    }
    Ok(Some(reply))
}

/// 回包信封（wire 契约，H2）：响应动作 JSON → `Message::SessionControl`
///
/// - 信封 `session_id` 取自响应动作 JSON 的 `session_id` 字段（start = 新会话
///   id、stop/remove = 回显 id、list = 无）；旧宿主路径的该字段语义逐字保留；
/// - 动作反序列化进 `SessionControlAction`（未知响应变体 → 显性报错，不静默吞）。
fn wrap_response(message_id: &str, reply: serde_json::Value) -> Result<Message> {
    let envelope_session_id = reply
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let action = serde_json::from_value::<SessionControlAction>(reply).map_err(|e| {
        AppError::Plugin(format!("session control reply is not a SessionControlAction: {e}"))
    })?;
    Ok(Message::SessionControl {
        message_id: message_id.to_string(),
        expect_response: false,
        session_id: envelope_session_id,
        timestamp: chrono::Utc::now().timestamp_millis(),
        token: String::new(),
        payload: SessionControlPayload { action },
    })
}

/// 变更类响应变体判定（传输面契约：按响应 wire 变体分类，非业务解释）
///
/// 移动端操作成功后桌面前端刷新通知；旧宿主按请求动作分类（start/stop/remove
/// → 刷新，list/resize → 不刷新）——响应变体与请求一一对应，分类等价。
fn is_mutation_response(action: &SessionControlAction) -> bool {
    matches!(
        action,
        SessionControlAction::StartSession { .. }
            | SessionControlAction::StopSession { .. }
            | SessionControlAction::RemoveSession { .. }
    )
}

/// 桌面前端刷新通知（无 AppHandle 的无头/测试上下文跳过）
fn emit_refresh(app_handle: &Option<Arc<AppHandle>>, device_name: Option<&str>) {
    let Some(handle) = app_handle else { return };
    let source = device_name.unwrap_or("mobile").to_string();
    if let Err(e) = handle.emit(
        "sessions-refresh",
        RefreshEvent {
            refresh_type: "sessions".to_string(),
            source: source.clone(),
        },
    ) {
        tracing::error!(error = %e, "Failed to emit sessions-refresh event");
    }
    tracing::info!(source = %source, "[SessionControl] Emitted sessions-refresh event");
}

/// 处理完整的 Control 消息（路由层）
///
/// 会话真源在插件登记域：本层只做「转发 + 回包信封」。插件未激活 / 端点未声明
/// 时 `relay_action` 返回显性错误（不静默降级）——移动端经 `Message::error` 可见。
///
/// 参数即 WS 分派入口收到的 wire 字段（host_ctx 为插件上下文），打包结构体
/// 只会让 event.rs 调用点更绕；分派入口按字段透传，允许参数清单。
#[allow(clippy::too_many_arguments)]
pub async fn handle_control_message(
    host_ctx: &Arc<WasmHostContext>,
    message_id: String,
    _session_id: Option<String>,
    _timestamp: i64,
    action: SessionControlAction,
    _addr: SocketAddr,
    device_name: Option<String>,
    app_handle: Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    // 转发（不解动作名语义；未声明/未激活 → 显性 Err）
    let reply = relay_action(host_ctx, &action, device_name.as_deref()).await?;
    let Some(reply) = reply else {
        // 无回包动作（resize）：fire-and-forget，不构造响应
        return Ok(None);
    };

    let wrapped = wrap_response(&message_id, reply)?;

    // 桌面前端刷新通知（按响应变体分类——传输面契约）
    if let Message::SessionControl { payload, .. } = &wrapped {
        if is_mutation_response(&payload.action) {
            emit_refresh(&app_handle, device_name.as_deref());
        }
    }

    Ok(Some(wrapped))
}

// ==================== Tests ====================

/// 会话控制端点表的测试串行锁（声明闸门用例与转发闭环用例共用）
///
/// 端点注册表是全局单例：声明闸门用例（本文件）与转发闭环用例
/// （`wasm_core` session_e2e）都注册/摘除 `com.bedcode.terminal-session/`
/// `session-control`，并行会互相打断注册态。同一把锁串行化两端点用例。
#[cfg(test)]
pub(crate) static SESSION_CONTROL_ENDPOINT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 回包信封（纯函数契约） ====================

    /// start 响应：信封 session_id = 动作 JSON 的 session_id（新会话 id），
    /// 动作反序列化只取 config_id（额外字段剥除，不出网）
    #[test]
    fn wrap_response_start_uses_reply_session_id() {
        let msg = wrap_response(
            "m1",
            serde_json::json!({
                "type": "start_session",
                "config_id": "c1",
                "session_id": "sess-new",
            }),
        )
        .expect("wrap");
        match msg {
            Message::SessionControl { message_id, session_id, payload, .. } => {
                assert_eq!(message_id, "m1");
                assert_eq!(session_id.as_deref(), Some("sess-new"));
                assert!(matches!(
                    payload.action,
                    SessionControlAction::StartSession { ref config_id } if config_id == "c1"
                ));
            }
            other => panic!("expected SessionControl, got: {other:?}"),
        }
    }

    /// list 响应：无 session_id 字段 → 信封 None（旧宿主 ListSessions 同形状）
    #[test]
    fn wrap_response_list_has_no_envelope_session() {
        let msg = wrap_response("m2", serde_json::json!({ "type": "session_list", "sessions": [] })).expect("wrap");
        match msg {
            Message::SessionControl { session_id, payload, .. } => {
                assert!(session_id.is_none());
                assert!(matches!(payload.action, SessionControlAction::SessionList { .. }));
            }
            other => panic!("expected SessionControl, got: {other:?}"),
        }
    }

    /// 未知响应变体显性报错（协议错位不得静默吞）
    #[test]
    fn wrap_response_rejects_unknown_variant() {
        let err = wrap_response("m3", serde_json::json!({ "type": "bogus_reply" }))
            .expect_err("unknown reply variant must fail");
        assert!(err.to_string().contains("bogus_reply"), "got: {err}");
    }

    // ==================== 变更分类（传输面契约） ====================

    #[test]
    fn mutation_classification_matches_legacy_refresh_set() {
        // 旧宿主：start/stop/remove → sessions-refresh；list/resize → 不刷新
        assert!(is_mutation_response(&SessionControlAction::StartSession { config_id: "c".into() }));
        assert!(is_mutation_response(&SessionControlAction::StopSession { session_id: "s".into() }));
        assert!(is_mutation_response(&SessionControlAction::RemoveSession { session_id: "s".into() }));
        assert!(!is_mutation_response(&SessionControlAction::ListSessions));
        assert!(!is_mutation_response(&SessionControlAction::ResizeSession {
            session_id: "s".into(),
            cols: 120,
            rows: 40,
            force: false,
        }));
        assert!(!is_mutation_response(&SessionControlAction::SessionChanged {
            change_type: "created".into(),
            session: crate::enums::SessionSummary {
                id: "s".into(),
                name: "n".into(),
                status: "running".into(),
                created_at: "t".into(),
                started_at: None,
                session_type: None,
                config_id: None,
                task_status: None,
                task_reason: None,
            },
        }));
    }

    // ==================== 声明闸门 ====================

    /// 闸门判据：端点未注册 → 未声明（含初始态）；注册 → 已声明；摘除 → 恢复未声明
    #[test]
    fn declaration_gate_tracks_endpoint_registry() {
        use crate::server::websocket::endpoint::{purge_for_plugin, register};
        use crate::wasm_core::bus::MessageBus;
        use std::sync::Arc;

        // 与转发闭环用例（session_e2e）串行：端点表是全局单例，两端点用例互不打断
        let _gate_guard = SESSION_CONTROL_ENDPOINT_TEST_LOCK.lock().unwrap();

        // 清理同属主残留（全局表跨用例共享）
        purge_for_plugin(SESSION_CONTROL_PLUGIN);

        assert!(!session_control_declared(), "未注册 → 未声明");
        let entry = register(
            SESSION_CONTROL_PLUGIN,
            SESSION_CONTROL_PATH,
            bedcode_plugin_api::EndpointAuth::Jwt,
            None,
            None,
            Arc::new(MessageBus::new()),
        )
        .expect("register declared endpoint");
        assert!(session_control_declared(), "注册后 → 已声明");
        assert!(purge_for_plugin(SESSION_CONTROL_PLUGIN).iter().any(|e| e.endpoint_id == entry.endpoint_id));
        assert!(!session_control_declared(), "摘除后 → 恢复未声明");
    }
}
