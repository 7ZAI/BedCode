//! Authentication Service
//!
//! 处理设备配对和认证逻辑

use crate::desktop::server::connection_types::{AuthPayload, AuthStage, DeviceConnectionEvent, PairingCodeGeneratedEvent};
use crate::desktop::server::message::Message;
use crate::desktop::server::services::pairing_service::PairingService;
use crate::desktop::server::services::qr_token_service::QrTokenService;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::shared::auth::JwtService;
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
    db: &Arc<Mutex<Database>>,
    pairing_service: &Arc<PairingService>,
    qr_manager: &Arc<QrTokenService>,
    jwt_service: &JwtService,
    ws_manager: &WebSocketManager,
    app_handle: &Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    tracing::info!("handle_auth called with stage: {:?}", payload.stage);
    match payload.stage {
        AuthStage::RequestPairing => {
            let existing_code = pairing_service.get_current_code().await;
            let code = if let Some(ref existing) = existing_code {
                if !existing.is_expired() {
                    tracing::info!("Reusing existing pairing code: {} for device {:?}", existing.code, payload.device_name);
                    existing.clone()
                } else {
                    pairing_service.generate_code().await
                }
            } else {
                pairing_service.generate_code().await
            };

            tracing::info!("Pairing requested by device {:?} ({:?}), code: {}", payload.device_id, payload.device_name, code.code);

            if let Some(handle) = app_handle {
                let event = PairingCodeGeneratedEvent {
                    code: code.code.clone(),
                    expires_in: code.expires_in,
                    device_name: payload.device_name.clone(),
                };
                if let Err(e) = handle.emit("pairing-code-generated", &event) {
                    tracing::error!("Failed to emit pairing code event: {}", e);
                }
            }

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
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
            let is_valid = pairing_service.verify_code(&code).await;

            if is_valid {
                let device_name = payload.device_name.clone().unwrap_or_else(|| "Unknown Device".to_string());
                let device_name_for_client = payload.device_name.clone();
                let fingerprint = payload.device_fingerprint.unwrap_or_default();
                let address = format!("{}", addr);

                // 生成 JWT token
                let session_token = jwt_service.generate_token(
                    "pending".to_string(),
                    Some(device_name.clone()),
                    Some(fingerprint.clone()),
                ).map_err(|e| crate::AppError::Auth(e.to_string()))?;

                // 使用 WebSocketManager 设置真正的客户端认证状态
                ws_manager.set_authenticated(&addr, Some("pending".to_string())).await;
                if let Some(ref name) = device_name_for_client {
                    ws_manager.set_device_name(&addr, Some(name.clone())).await;
                }

                if let Some(handle) = app_handle {
                    let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                        addr: addr.to_string(),
                        device_id: "pending".to_string(),
                        device_name: payload.device_name.clone(),
                        event: "authenticated".to_string(),
                    });
                }

                pairing_service.clear_code().await;
                tracing::info!("Device paired: {} (fingerprint: {}, addr: {})", device_name, fingerprint, address);

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Authenticated,
                        device_id: Some("pending".to_string()),
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
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Failed,
                        error: Some(error_message.to_string()),
                        ..Default::default()
                    },
                }))
            }
        }

        AuthStage::Authenticated => {
            let device_id = payload.device_id.unwrap_or_default();
            let fingerprint = payload.device_fingerprint.unwrap_or_default();
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
            ws_manager.set_authenticated(&addr, Some(claims.sub.clone())).await;
            if let Some(name) = &payload.device_name {
                ws_manager.set_device_name(&addr, Some(name.clone())).await;
            }

            if let Some(handle) = app_handle {
                let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                    addr: addr.to_string(),
                    device_id: claims.sub.clone(),
                    device_name: payload.device_name.clone(),
                    event: "authenticated".to_string(),
                });
            }

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
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
                    let device_fingerprint = payload.device_fingerprint.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let device_name = payload.device_name.clone().unwrap_or_else(|| "QR Device".to_string());

                    // 生成 JWT token
                    let session_token = jwt_service.generate_token(
                        "pending".to_string(),
                        Some(device_name.clone()),
                        Some(device_fingerprint.clone()),
                    ).map_err(|e| crate::AppError::Auth(e.to_string()))?;

                    // 使用 WebSocketManager 设置真正的客户端认证状态
                    ws_manager.set_authenticated(&addr, Some("pending".to_string())).await;
                    ws_manager.set_device_name(&addr, Some(device_name.clone())).await;

                    if let Some(handle) = app_handle {
                        let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                            addr: addr.to_string(),
                            device_id: "pending".to_string(),
                            device_name: Some(device_name.clone()),
                            event: "authenticated".to_string(),
                        });
                    }

                    let response = Message::Auth {
                        message_id: request_message_id,
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        payload: AuthPayload {
                            stage: AuthStage::Authenticated,
                            device_id: Some("pending".to_string()),
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
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
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