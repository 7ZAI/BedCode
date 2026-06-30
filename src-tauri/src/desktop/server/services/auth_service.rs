//! Authentication Service
//!
//! 处理设备配对和认证逻辑

use crate::desktop::server::connection_types::{AuthPayload, AuthStage, DeviceConnectionEvent, PairingCodeGeneratedEvent};
use crate::desktop::server::message::Message;
use crate::desktop::server::services::pairing_service::PairingService;
use crate::desktop::auth::qr_token::QrTokenManager;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::desktop::auth::JwtService;
use crate::shared::db::Database;
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::Emitter;
use tauri::AppHandle;
use tokio::sync::Mutex;


/// 处理认证消息
pub async fn handle_auth(
    payload: AuthPayload,
    request_message_id: String,
    addr: SocketAddr,
    pairing_service: &Arc<PairingService>,
    qr_manager: &Arc<QrTokenManager>,
    jwt_service: &JwtService,
    ws_manager: &WebSocketManager,
    app_handle: &Option<Arc<AppHandle>>,
    db: &Arc<Mutex<Database>>,
) -> Result<Option<Message>> {
    tracing::info!("handle_auth called with stage: {:?}", payload.stage);
    match payload.stage {
        AuthStage::RequestPairing => {
            // 每次配对请求都生成新的配对码，不复用现有的
            // 原因：确保用户有足够时间输入，避免复用过期码导致混乱
            let code = pairing_service.generate_code().await;

            tracing::info!("Pairing requested by device {:?} ({:?}), code: {}", payload.device_id, payload.device_name, code.code);
            tracing::info!("app_handle is_some={}", app_handle.is_some());

            if let Some(handle) = app_handle {
                let event = PairingCodeGeneratedEvent {
                    code: code.code.clone(),
                    expires_in: code.remaining_seconds(),  // 使用剩余时间，而不是原始 TTL
                    device_name: payload.device_name.clone(),
                };
                tracing::info!("Emitting pairing-code-generated event: code={}", event.code);
                if let Err(e) = handle.emit("pairing-code-generated", &event) {
                    tracing::error!("Failed to emit pairing code event: {}", e);
                } else {
                    tracing::info!("pairing-code-generated event emitted successfully");
                }
            } else {
                tracing::warn!("app_handle is None, cannot emit pairing-code-generated event");
            }

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::VerifyCode,
                    device_id: payload.device_id,
                    device_name: payload.device_name,
                    error: None,
                    ..Default::default()
                },
            }))
        }

        AuthStage::VerifyCode => {
            let code = payload.pairing_code.unwrap_or_default();
            // 使用新方法：验证并消耗配对码（单次使用）
            let is_valid = pairing_service.verify_and_consume_code(&code).await;

            if is_valid {
                let device_name = payload.device_name.clone().unwrap_or_else(|| "Unknown Device".to_string());
                let device_name_for_client = payload.device_name.clone();
                let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
                let address = format!("{}", addr);
                let device_id = payload.device_id.clone().unwrap_or_else(|| addr.to_string());

                // 生成 JWT token（使用真实的设备 ID）
                let session_token = jwt_service.generate_token(
                    device_id.clone(),
                    Some(device_name.clone()),
                    Some(fingerprint.clone()),
                ).map_err(|e| crate::AppError::Auth(e.to_string()))?;

                // 使用 WebSocketManager 设置真正的客户端认证状态
                ws_manager.set_authenticated(&addr, Some(device_id.clone()), Some(fingerprint.clone())).await;
                if let Some(ref name) = device_name_for_client {
                    ws_manager.set_device_name(&addr, Some(name.clone())).await;
                }

                // 记录/更新配对设备到数据库
                let display_name = format_device_display_name(&device_name, &address);
                {
                    let db_guard = db.lock().await;
                    if let Err(e) = db_guard.add_pairing(&display_name, &fingerprint, "", Some(&address)) {
                        tracing::warn!("Failed to record pairing for {}: {}", device_name, e);
                    }
                }

                if let Some(handle) = app_handle {
                    let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                        addr: addr.to_string(),
                        device_id: device_id.clone(),
                        device_name: payload.device_name.clone(),
                        fingerprint: Some(fingerprint.clone()),
                        event: "authenticated".to_string(),
                    });
                }

                tracing::info!("Device paired: {} (fingerprint: {}, addr: {})", device_name, fingerprint, address);

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                expect_response: false,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    token: String::new(),
                    payload: AuthPayload {
                        stage: AuthStage::Authenticated,
                        device_id: Some(device_id),
                        device_fingerprint: Some(fingerprint),
                        session_token: Some(session_token),
                        error: None,
                        ..Default::default()
                    },
                }))
            } else {
                let current_code = pairing_service.get_current_code().await;
                let error_message = if current_code.is_none() {
                    "No pairing code available. Please generate a new code."
                } else {
                    "Invalid or expired pairing code"
                };

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                expect_response: false,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    token: String::new(),
                    payload: AuthPayload {
                        stage: AuthStage::Failed,
                        error: Some(error_message.to_string()),
                        ..Default::default()
                    },
                }))
            }
        }

        AuthStage::Authenticated | AuthStage::Reauthenticate => {
            let device_id = payload.device_id.unwrap_or_default();
            let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
            let token = payload.session_token.unwrap_or_default();

            // 直接验证 JWT token，不使用数据库
            let claims = match jwt_service.verify_token_with_expiry(&token) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("JWT verification failed: {:?}", e);
                    return Ok(Some(Message::error_with_id(&request_message_id, "INVALID_TOKEN", "Invalid or expired token")));
                }
            };

            tracing::info!("Device re-authenticated: {} (sub: {})", addr, claims.sub);

            // 使用 WebSocketManager 设置真正的客户端认证状态
            ws_manager.set_authenticated(&addr, Some(claims.sub.clone()), Some(fingerprint.clone())).await;
            if let Some(name) = &payload.device_name {
                ws_manager.set_device_name(&addr, Some(name.clone())).await;
            }

            // 更新配对设备的 last_seen 和 connect_count
            if !fingerprint.is_empty() {
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.update_pairing_last_seen(&fingerprint) {
                    tracing::warn!("Failed to update pairing last_seen for {}: {}", fingerprint, e);
                }
            }

            if let Some(handle) = app_handle {
                let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                    addr: addr.to_string(),
                    device_id: claims.sub.clone(),
                    device_name: payload.device_name.clone(),
                    fingerprint: Some(fingerprint.clone()),
                    event: "authenticated".to_string(),
                });
            }

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Authenticated,
                    device_id: Some(claims.sub),
                    device_fingerprint: Some(fingerprint),
                    session_token: Some(token),
                    error: None,
                    ..Default::default()
                },
            }))
        }

        AuthStage::QrConnect => {
            let qr_token = payload.qr_token.as_deref().unwrap_or("");
            tracing::info!("QR connect request from {} with token length: {}", addr, qr_token.len());

            match qr_manager.verify(qr_token).await {
                Ok(()) => {
                    // QR token 已消耗，通知桌面前端重新生成
                    if let Some(handle) = app_handle {
                        let _ = handle.emit("qr-token-consumed", ());
                    }

                    let device_fingerprint = payload.device_fingerprint.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let device_name = payload.device_name.clone().unwrap_or_else(|| "QR Device".to_string());
                    let device_id = payload.device_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let address = format!("{}", addr);

                    // 生成 JWT token（使用真实的设备 ID）
                    let session_token = jwt_service.generate_token(
                        device_id.clone(),
                        Some(device_name.clone()),
                        Some(device_fingerprint.clone()),
                    ).map_err(|e| crate::AppError::Auth(e.to_string()))?;

                    // 使用 WebSocketManager 设置真正的客户端认证状态
                    ws_manager.set_authenticated(&addr, Some(device_id.clone()), Some(device_fingerprint.clone())).await;
                    ws_manager.set_device_name(&addr, Some(device_name.clone())).await;

                    // 记录/更新配对设备到数据库
                    let display_name = format_device_display_name(&device_name, &address);
                    {
                        let db_guard = db.lock().await;
                        if let Err(e) = db_guard.add_pairing(&display_name, &device_fingerprint, "", Some(&address)) {
                            tracing::warn!("Failed to record pairing for {}: {}", device_name, e);
                        }
                    }

                    if let Some(handle) = app_handle {
                        let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                            addr: addr.to_string(),
                            device_id: device_id.clone(),
                            device_name: Some(device_name.clone()),
                            fingerprint: Some(device_fingerprint.clone()),
                            event: "authenticated".to_string(),
                        });
                    }

                    let response = Message::Auth {
                        message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Authenticated,
                    device_id: Some(device_id),
                    device_fingerprint: Some(device_fingerprint),
                    session_token: Some(session_token),
                    device_name: Some(device_name),
                    pairing_code: None,
                    error: None,
                    qr_token: None,
                },
            };
                    Ok(Some(response))
                }
                Err(e) => {
                    tracing::warn!("QR token verification failed from {}: {}", addr, e);
                    let error_msg = e.to_string();
                    let user_message = if error_msg.contains("expired") {
                        "二维码已过期，请重新生成".to_string()
                    } else if error_msg.contains("already used") {
                        "二维码已绑定其他设备，请重新扫描".to_string()
                    } else if error_msg.contains("No active QR token") {
                        "请先在桌面端生成二维码".to_string()
                    } else if error_msg.contains("Invalid QR token") {
                        "无效的二维码，请重新扫描".to_string()
                    } else {
                        error_msg
                    };
                    let response = Message::Auth {
                        message_id: request_message_id,
                expect_response: false,
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        token: String::new(),
                        payload: AuthPayload {
                            stage: AuthStage::QrFailed,
                            error: Some(user_message),
                            device_id: None,
                            device_fingerprint: None,
                            session_token: None,
                            device_name: None,
                            pairing_code: None,
                            qr_token: None,
                        },
                    };
                    Ok(Some(response))
                }
            }
        }

        _ => Ok(Some(Message::error_with_id(&request_message_id, "INVALID_AUTH_STAGE", "Invalid auth stage"))),
    }
}

/// 处理 JWT Token 认证（已认证客户端的 JWT 验证）
pub async fn handle_jwt_auth(
    request_message_id: String,
    session_id: Option<String>,
    timestamp: i64,
    payload: AuthPayload,
    addr: SocketAddr,
    jwt_service: &JwtService,
    app_handle: &Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    // 从 payload 中获取 JWT token
    let token = match &payload.session_token {
        Some(t) if !t.is_empty() => t.clone(),
        _ => {
            // 没有 token，返回认证失败
            return Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id,
                timestamp,
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Failed,
                    error: Some("No JWT token provided".to_string()),
                    ..Default::default()
                },
            }));
        }
    };

    // 验证 JWT
    match jwt_service.verify_token_with_expiry(&token) {
        Ok(claims) => {
            // JWT 验证成功，使用 WebSocketManager 设置客户端为已认证
            let ws_manager = WebSocketManager::global();
            ws_manager.set_authenticated(&addr, Some(claims.sub.clone()), claims.fingerprint.clone()).await;
            if let Some(name) = &claims.device_name {
                ws_manager.set_device_name(&addr, Some(name.clone())).await;
            }

            // 发送认证成功事件给前端
            if let Some(handle) = app_handle {
                let event = DeviceConnectionEvent {
                    addr: addr.to_string(),
                    device_id: claims.sub.clone(),
                    device_name: claims.device_name.clone(),
                    fingerprint: claims.fingerprint.clone(),
                    event: "authenticated".to_string(),
                };
                let _ = handle.emit("device-connected", &event);
            }

            tracing::info!(
                "Client {} authenticated via JWT (device: {})",
                addr,
                claims.sub
            );

            // 返回认证成功响应
            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id,
                timestamp,
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Authenticated,
                    device_id: Some(claims.sub),
                    device_name: claims.device_name,
                    device_fingerprint: claims.fingerprint,
                    session_token: Some(token),
                    error: None,
                    ..Default::default()
                },
            }))
        }
        Err(e) => {
            tracing::warn!("JWT verification failed for {}: {}", addr, e);

            // 返回认证失败响应
            let error_msg = match e {
                crate::desktop::auth::JwtError::TokenExpired => "JWT token expired, please re-authenticate",
                _ => "Invalid JWT token",
            };

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id,
                timestamp,
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Failed,
                    error: Some(error_msg.to_string()),
                    ..Default::default()
                },
            }))
        }
    }
}

/// 格式化设备显示名称：名称 + 首次连接 IP
pub fn format_device_display_name(device_name: &str, address: &str) -> String {
    // address 格式为 "IP:PORT"，提取 IP 部分
    let ip = address.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(address);
    format!("{} ({})", device_name, ip)
}