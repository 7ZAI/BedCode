//! Session Control Service
//!
//! 处理会话启动/停止/缩放等控制逻辑
//! 终端输出订阅统一走 TerminalAction::Subscribe 路径（JoinSession 链已删除）

use crate::enums::{SessionControlAction, SessionSummary};
use crate::server::websocket::message::Message;
use crate::protocol::RendererSource;
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 刷新事件类型
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshEvent {
    pub refresh_type: String,
    pub source: String,
}

/// 处理控制消息
pub async fn handle_control(
    action: SessionControlAction,
    request_message_id: String,
    // 票 11：`addr` 参数随内核会话面一并删除（它只被内核分支用于按客户端退订）
    device_name: Option<String>,
) -> Result<Option<Message>> {
    // P1-b 起会话真源在插件登记域：所有动作经宿主窄转发层调插件互调 api
    let host_ctx = || crate::system::app_context::AppContext::global().plugin_host().wasm_host_ctx();
    match action {
        SessionControlAction::ListSessions => {
            // 票 12：任务字段取自注解槽；P1-b 起真源在插件登记域
            let sessions = crate::utils::session_gateway::list_views(host_ctx()).await?;

            let all_sessions: Vec<SessionSummary> = sessions
                .into_iter()
                .map(|s| SessionSummary {
                    id: s.info.id,
                    name: s.info.name,
                    status: serde_json::to_value(&s.info.status)
                        .and_then(|v| serde_json::from_value::<String>(v))
                        .unwrap_or_else(|_| format!("{:?}", s.info.status)),
                    created_at: s.info.created_at.to_rfc3339(),
                    started_at: s.info.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("pty".to_string()),
                    config_id: Some(s.info.config_id),
                    task_status: s.task_status,
                    task_reason: s.task_reason,
                })
                .collect();

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::enums::SessionControlPayload {
                    action: SessionControlAction::SessionList { sessions: all_sessions },
                },
            }))
        }

        SessionControlAction::StartSession { config_id } => {
            // host-business-decarriage 收尾：WS 控制路径与 HTTP 启动线同源——创建编排
            // 走会话中心插件（插件必需，无宿主降级）。响应消息形状不变。
            // WS 控制路径未携带初始尺寸（协议未扩展）：None → 用配置默认值；
            // 移动端 UI 实际走 HTTP start（携带终端组件默认网格）。
            let session_id = crate::utils::session_gateway::start(
                crate::system::app_context::AppContext::global()
                    .plugin_host()
                    .wasm_host_ctx(),
                &config_id,
                None,
                None,
                true,
                device_name.as_deref(),
            )
            .await?;
            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::enums::SessionControlPayload {
                    action: SessionControlAction::StartSession { config_id },
                },
            }))
        }

        SessionControlAction::StopSession { session_id } => {
            crate::utils::session_gateway::stop(host_ctx(), &session_id, device_name.clone()).await?;

            // 取消该客户端对此会话的输出订阅
            // 票 11：退订由连接侧随引擎终态帧自理（见 `session_gateway` 同处注释）

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::enums::SessionControlPayload {
                    action: SessionControlAction::StopSession { session_id },
                },
            }))
        }

        SessionControlAction::RemoveSession { session_id } => {
            crate::utils::session_gateway::remove(host_ctx(), &session_id, device_name.clone()).await?;

            // 取消该客户端对此会话的输出订阅
            // 票 11：退订由连接侧随引擎终态帧自理（见 `session_gateway` 同处注释）

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::enums::SessionControlPayload {
                    action: SessionControlAction::RemoveSession { session_id },
                },
            }))
        }

        SessionControlAction::ResizeSession {
            session_id,
            cols,
            rows,
            force,
        } => {
            // 更新 PTY 尺寸，使输出按实际渲染端排版。
            //
            // 桌面端 PTY 的尺寸由正统渲染端决定（每会话唯一归属，见
            // SessionManager::resize_session 裁决）：请求方非正统且未 force 时
            // 返回 NeedsConfirmation（此处仅记日志，客户端弹窗确认后带 force
            // 重发或改用 HTTP 路径）。桌面本地 resize 默认经 Tauri 命令路径。
            let source = match device_name.clone() {
                Some(name) => RendererSource::Mobile { device_name: name },
                None => {
                    tracing::warn!(
                        session_id = %session_id,
                        "WS resize without device_name claims, treating as Desktop source"
                    );
                    RendererSource::Desktop
                }
            };
            if let Err(e) = crate::utils::session_gateway::resize(host_ctx(), &session_id, cols, rows, source, force)
                .await
            {
                tracing::warn!(error = %e, session_id = %session_id, "Failed to resize PTY session");
            }

            Ok(None)
        }

        _ => Ok(None),
    }
}

/// 处理完整的 Control 消息（路由层）
///
/// 票 11：`session_manager` 参数删除——它此前只作「宿主会话内核是否可用」的存在性门
/// （内层 `handle_control` 一律走插件互调 api，从不读它）。会话真源在插件登记域，
/// 宿主这一侧没有「不可用」态：不可用表现为插件未激活 → 内层返回显性错误。
pub async fn handle_control_message(
    message_id: String,
    _session_id: Option<String>,
    _timestamp: i64,
    action: SessionControlAction,
    addr: SocketAddr,
    device_name: Option<String>,
    app_handle: Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    match action {
        SessionControlAction::ListSessions
        | SessionControlAction::StartSession { .. }
        | SessionControlAction::StopSession { .. }
        | SessionControlAction::ResizeSession { .. }
        | SessionControlAction::RemoveSession { .. } => {
            let result = handle_control(action.clone(), message_id, device_name.clone()).await?;

            // 移动端操作成功后，发送刷新事件通知桌面端前端
            if let Some(handle) = app_handle {
                let source = device_name.unwrap_or_else(|| "mobile".to_string());
                match &action {
                    SessionControlAction::StopSession { .. } | SessionControlAction::RemoveSession { .. } => {
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
                    SessionControlAction::StartSession { .. } => {
                        if let Err(e) = handle.emit(
                            "sessions-refresh",
                            RefreshEvent {
                                refresh_type: "sessions".to_string(),
                                source: source.clone(),
                            },
                        ) {
                            tracing::error!(error = %e, "Failed to emit sessions-refresh event");
                        }
                        tracing::info!("[SessionControl] Emitted sessions-refresh event from {}", source);
                    }
                    _ => {}
                }
            }

            Ok(result)
        }
        _ => {
            tracing::debug!("Unhandled control action: {:?}", action);
            Ok(None)
        }
    }
}

// ==================== Output Buffer ====================
