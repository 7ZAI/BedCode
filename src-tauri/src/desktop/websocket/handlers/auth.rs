//! Authentication Handler
//!
//! 处理设备配对和认证逻辑

use crate::shared::auth::PairingService;
use crate::shared::auth::QrTokenManager;
use crate::shared::db::Database;
use crate::desktop::websocket::message::{AuthPayload, AuthStage, Message, PairingCodeGeneratedEvent, DeviceConnectionEvent};
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tauri::Emitter;
use tauri::AppHandle;
use uuid::Uuid;

/// 处理认证消息
pub async fn handle_auth(
    payload: AuthPayload,
    request_message_id: String,
    addr: SocketAddr,
    db: &Arc<Mutex<Database>>,
    pairing_service: &Arc<PairingService>,
    qr_manager: &Arc<QrTokenManager>,
    clients: &Arc<RwLock<HashMap<SocketAddr, crate::desktop::websocket::server::ClientInfo>>>,
    app_handle: &Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    tracing::info!("handle_auth called with stage: {:?}", payload.stage);
    match payload.stage {
        AuthStage::RequestPairing => {
            // 检查是否已有活跃的配对码，避免覆盖
            let existing_code = pairing_service.get_current_code().await;
            let code = if let Some(ref existing) = existing_code {
                if !existing.is_expired() {
                    tracing::info!(
                        "Reusing existing pairing code: {} for device {:?}",
                        existing.code,
                        payload.device_name
                    );
                    existing.clone()
                } else {
                    pairing_service.generate_code().await
                }
            } else {
                pairing_service.generate_code().await
            };

            tracing::info!(
                "Pairing requested by device {:?} ({:?}), code: {}",
                payload.device_id,
                payload.device_name,
                code.code
            );

            // 发送事件到桌面端前端，显示配对码
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
                let session_token = Uuid::new_v4().to_string();

                let db = db.lock().await;
                let pairing_id = db.add_pairing(&device_name, &fingerprint, "", Some(&address))?;
                db.update_pairing_token(&pairing_id, &session_token)?;
                drop(db);

                // 更新客户端状态
                {
                    let mut clients = clients.write().await;
                    if let Some(client) = clients.get_mut(&addr) {
                        client.device_id = Some(pairing_id.clone());
                        client.device_name = device_name_for_client;
                        client.authenticated = true;
                    }
                }

                // 通知前端设备已认证
                if let Some(handle) = app_handle {
                    let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                        addr: addr.to_string(),
                        device_id: pairing_id.clone(),
                        device_name: payload.device_name.clone(),
                        event: "authenticated".to_string(),
                    });
                }

                // 清除已使用的配对码
                pairing_service.clear_code().await;

                tracing::info!("Device paired: {} (fingerprint: {}, addr: {})", device_name, fingerprint, address);

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Authenticated,
                        device_id: Some(pairing_id),
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

            let db = db.lock().await;
            let pairings = db.get_pairings()?;

            // 通过 fingerprint + token 验证已配对设备
            // 也兼容通过 pairing id 匹配
            let is_paired = if !fingerprint.is_empty() && !token.is_empty() {
                pairings.iter().any(|p| p.device_fingerprint == fingerprint
                    && p.session_token.as_deref() == Some(&token)
                    && p.is_active)
            } else if !device_id.is_empty() {
                pairings.iter().any(|p| p.id == device_id && p.is_active)
            } else {
                false
            };
            drop(db);

            if is_paired {
                // 更新客户端认证状态
                {
                    let mut clients = clients.write().await;
                    if let Some(client) = clients.get_mut(&addr) {
                        client.device_id = Some(device_id.clone());
                        client.device_name = payload.device_name.clone();
                        client.authenticated = true;
                    }
                }

                // 通知前端设备已重新认证
                if let Some(handle) = app_handle {
                    let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                        addr: addr.to_string(),
                        device_id: device_id.clone(),
                        device_name: payload.device_name.clone(),
                        event: "authenticated".to_string(),
                    });
                }

                tracing::info!("Device re-authenticated: {}", addr);

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Authenticated,
                        device_id: Some(device_id),
                        device_fingerprint: Some(fingerprint),
                        session_token: Some(token),
                        error: None,
                        ..Default::default()
                    },
                }))
            } else {
                tracing::warn!("Authentication failed for device {}", addr);
                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Failed,
                        error: Some("Device not paired or invalid credentials".to_string()),
                        ..Default::default()
                    },
                }))
            }
        }

        AuthStage::QrConnect => {
            let qr_token = payload.qr_token.as_deref().unwrap_or("");

            tracing::info!("QR connect request from {} with token length: {}", addr, qr_token.len());
            tracing::debug!("QR connect payload: device_id={:?}, device_name={:?}, qr_token={}",
                payload.device_id, payload.device_name, qr_token);

            match qr_manager.verify(qr_token).await {
                Ok(()) => {
                    // 生成设备凭证
                    let device_id = Uuid::new_v4().to_string();
                    let device_fingerprint = payload.device_fingerprint
                        .clone()
                        .unwrap_or_else(|| Uuid::new_v4().to_string());
                    let device_name = payload.device_name
                        .clone()
                        .unwrap_or_else(|| "QR Device".to_string());
                    let session_token = Uuid::new_v4().to_string();

                    // 创建配对记录并保存 session_token
                    let db = db.lock().await;
                    let pairing_id = db.add_pairing(
                        &device_name,
                        &device_fingerprint,
                        "",
                        Some(&addr.to_string()),
                    )?;
                    db.update_pairing_token(&pairing_id, &session_token)?;
                    drop(db);

                    // 标记客户端已认证
                    {
                        let mut clients = clients.write().await;
                        if let Some(client) = clients.get_mut(&addr) {
                            client.authenticated = true;
                            client.device_id = Some(device_id.clone());
                            client.device_name = Some(device_name.clone());
                        }
                    }

                    // 通知前端设备已通过 QR 认证
                    if let Some(handle) = app_handle {
                        let _ = handle.emit("device-connected", &DeviceConnectionEvent {
                            addr: addr.to_string(),
                            device_id: device_id.clone(),
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
                    // 根据错误类型返回更友好的错误信息
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

// 为 AuthPayload 实现 Default
impl Default for AuthPayload {
    fn default() -> Self {
        Self {
            stage: AuthStage::RequestPairing,
            device_id: None,
            device_name: None,
            device_fingerprint: None,
            pairing_code: None,
            session_token: None,
            error: None,
            qr_token: None,
        }
    }
}