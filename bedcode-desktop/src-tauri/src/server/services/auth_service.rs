//! Authentication Service
//!
//! 处理设备配对和认证逻辑

use crate::server::connection_types::{AuthPayload, AuthStage, DeviceConnectionEvent, PairingCodeGeneratedEvent};
use crate::server::message::Message;
use crate::server::services::pairing_service::PairingService;
use crate::utils::auth::qr_token::QrTokenManager;
use crate::server::ws::WebSocketManager;
use crate::utils::auth::JwtService;
use crate::utils::auth::biometric::verify_biometric_signature;
use crate::system::app_context::AppContext;
use crate::system::constants::event;
use crate::db::{Database, connection_method, connection_result};
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::Emitter;
use tauri::AppHandle;
use tokio::sync::Mutex;

/// 记录连接历史事件（未配对设备忽略）
async fn record_history(
    db: &Arc<Mutex<Database>>,
    fingerprint: &str,
    auth_method: &str,
    result: &str,
    address: Option<&str>,
) {
    if fingerprint.is_empty() {
        return;
    }
    let db_guard = db.lock().await;
    if let Err(e) = db_guard.record_connection_event_by_fingerprint(fingerprint, auth_method, result, address) {
        tracing::warn!(fingerprint = %fingerprint, error = %e, "Failed to record connection history");
    }
}


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
                    tracing::error!(error = %e, "Failed to emit pairing code event");
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
                        tracing::warn!(device_name = %device_name, error = %e, "Failed to record pairing");
                    }
                }

                // 记录连接历史（配对码认证成功）
                record_history(db, &fingerprint, connection_method::PAIRING_CODE, connection_result::SUCCESS, Some(&address)).await;

                if let Some(handle) = app_handle {
                    let _ = handle.emit(event::DEVICE_CONNECTED, &DeviceConnectionEvent {
                        addr: addr.to_string(),
                        device_id: device_id.clone(),
                        device_name: payload.device_name.clone(),
                        fingerprint: Some(fingerprint.clone()),
                        event: "authenticated".to_string(),
                    });
                }

                tracing::info!(device_name = %device_name, fingerprint = %fingerprint, addr = %address, "Device paired");

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

                // 记录连接历史（配对码认证失败）
                let address = format!("{}", addr);
                let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
                record_history(db, &fingerprint, connection_method::PAIRING_CODE, connection_result::FAILED, Some(&address)).await;

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
                    // 记录连接历史（JWT 静默重连失败）
                    let address = format!("{}", addr);
                    record_history(db, &fingerprint, connection_method::JWT, connection_result::FAILED, Some(&address)).await;
                    return Ok(Some(Message::error_with_id(&request_message_id, "INVALID_TOKEN", "Invalid or expired token")));
                }
            };

            tracing::info!("Device re-authenticated: {} (sub: {})", addr, claims.sub);

            // 使用 WebSocketManager 设置真正的客户端认证状态
            ws_manager.set_authenticated(&addr, Some(claims.sub.clone()), Some(fingerprint.clone())).await;
            if let Some(name) = &payload.device_name {
                ws_manager.set_device_name(&addr, Some(name.clone())).await;
            }

            // 记录连接历史（JWT 静默重连成功）
            let address = format!("{}", addr);
            record_history(db, &fingerprint, connection_method::JWT, connection_result::SUCCESS, Some(&address)).await;

            // 更新配对设备的 last_seen 和 connect_count，并同步设备展示名
            // （设备上报了真实设备名时刷新历史记录，避免旧名残留；空串视为未上报，保留原值）
            if !fingerprint.is_empty() {
                let display_name = payload
                    .device_name
                    .as_deref()
                    .filter(|n| !n.trim().is_empty())
                    .map(|n| format_device_display_name(n, &address));
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.update_pairing_last_seen(&fingerprint, display_name.as_deref()) {
                    tracing::warn!(fingerprint = %fingerprint, error = %e, "Failed to update pairing last_seen");
                }
            }

            if let Some(handle) = app_handle {
                let _ = handle.emit(event::DEVICE_CONNECTED, &DeviceConnectionEvent {
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
                            tracing::warn!(device_name = %device_name, error = %e, "Failed to record pairing");
                        }
                    }

                    // 记录连接历史（QR 认证成功）
                    record_history(db, &device_fingerprint, connection_method::QR, connection_result::SUCCESS, Some(&address)).await;

                    if let Some(handle) = app_handle {
                        let _ = handle.emit(event::DEVICE_CONNECTED, &DeviceConnectionEvent {
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
                    ..Default::default()
                },
            };
                    Ok(Some(response))
                }
                Err(e) => {
                    tracing::warn!(addr = %addr, error = %e, "QR token verification failed");
                    // 记录连接历史（QR 认证失败）
                    let address = format!("{}", addr);
                    let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
                    record_history(db, &fingerprint, connection_method::QR, connection_result::FAILED, Some(&address)).await;
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
                            ..Default::default()
                        },
                    };
                    Ok(Some(response))
                }
            }
        }

        AuthStage::ExchangeCertificate => {
            let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
            let public_key = payload.public_key.clone().unwrap_or_default();
            let address = format!("{}", addr);

            // 仅允许已认证连接管理生物凭证（绑定/解绑）
            let authenticated = ws_manager.get_client_by_addr(&addr).await
                .map(|c| c.authenticated)
                .unwrap_or(false);
            if !authenticated {
                tracing::warn!(addr = %addr, "ExchangeCertificate rejected: connection not authenticated");
                return Ok(Some(Message::error_with_id(&request_message_id, "NOT_AUTHENTICATED", "Connection not authenticated")));
            }

            let pairing = {
                let db_guard = db.lock().await;
                db_guard.get_pairing_by_fingerprint(&fingerprint)?
            };
            let Some(pairing) = pairing else {
                tracing::warn!(fingerprint = %fingerprint, "ExchangeCertificate rejected: device not paired");
                return Ok(Some(Message::error_with_id(&request_message_id, "NOT_PAIRED", "Device not paired")));
            };

            {
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.update_pairing_public_key(&pairing.id, &public_key) {
                    tracing::error!(pairing_id = %pairing.id, error = %e, "Failed to update pairing public key");
                    return Ok(Some(Message::error_with_id(&request_message_id, "DB_ERROR", "Failed to update credential")));
                }
            }

            let is_binding = !public_key.is_empty();
            tracing::info!(pairing_id = %pairing.id, binding = is_binding, "Biometric credential updated");

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Authenticated,
                    device_id: Some(pairing.id),
                    device_fingerprint: Some(fingerprint),
                    error: None,
                    ..Default::default()
                },
            }))
        }

        AuthStage::BiometricRequest => {
            let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
            let address = format!("{}", addr);

            // 校验设备已配对且已绑定公钥
            let binding_ready = {
                let db_guard = db.lock().await;
                match db_guard.get_pairing_by_fingerprint(&fingerprint)? {
                    Some(p) => !p.public_key.is_empty(),
                    None => false,
                }
            };

            if !binding_ready {
                tracing::warn!(fingerprint = %fingerprint, addr = %addr, "BiometricRequest rejected: no bound credential");
                record_history(db, &fingerprint, connection_method::BIOMETRIC, connection_result::FAILED, Some(&address)).await;
                return Ok(Some(Message::error_with_id(&request_message_id, "CREDENTIAL_NOT_BOUND", "Biometric credential not bound")));
            }

            // 下发一次性挑战值（按连接地址管理，60s 过期）
            let nonce = AppContext::global().biometric_challenges().generate(&addr.to_string()).await;
            tracing::info!(addr = %addr, "Biometric challenge issued");

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::BiometricChallenge,
                    device_id: payload.device_id,
                    device_fingerprint: Some(fingerprint),
                    challenge_nonce: Some(nonce),
                    error: None,
                    ..Default::default()
                },
            }))
        }

        AuthStage::BiometricVerify => {
            let fingerprint = payload.device_fingerprint.clone().unwrap_or_default();
            let nonce = payload.challenge_nonce.clone().unwrap_or_default();
            let signature = payload.signature.clone().unwrap_or_default();
            let address = format!("{}", addr);
            let addr_key = addr.to_string();

            // 1. 校验并消费挑战值（绑定连接、单次、未过期）
            if let Err(e) = AppContext::global().biometric_challenges().verify_and_consume(&addr_key, &nonce).await {
                tracing::warn!(addr = %addr, error = %e, "BiometricVerify rejected: challenge invalid");
                record_history(db, &fingerprint, connection_method::BIOMETRIC, connection_result::FAILED, Some(&address)).await;
                return Ok(Some(Message::error_with_id(&request_message_id, "CHALLENGE_INVALID", &e.to_string())));
            }

            // 2. 取配对记录与绑定的公钥
            let pairing = {
                let db_guard = db.lock().await;
                db_guard.get_pairing_by_fingerprint(&fingerprint)?
            };
            let Some(pairing) = pairing else {
                tracing::warn!(fingerprint = %fingerprint, "BiometricVerify rejected: device not paired");
                record_history(db, &fingerprint, connection_method::BIOMETRIC, connection_result::FAILED, Some(&address)).await;
                return Ok(Some(Message::error_with_id(&request_message_id, "NOT_PAIRED", "Device not paired")));
            };
            if pairing.public_key.is_empty() {
                tracing::warn!(fingerprint = %fingerprint, "BiometricVerify rejected: no bound credential");
                record_history(db, &fingerprint, connection_method::BIOMETRIC, connection_result::FAILED, Some(&address)).await;
                return Ok(Some(Message::error_with_id(&request_message_id, "CREDENTIAL_NOT_BOUND", "Biometric credential not bound")));
            }

            // 3. 验签（生物认证通过后由安全硬件签名）
            if let Err(e) = verify_biometric_signature(&pairing.public_key, &nonce, &signature) {
                tracing::warn!(fingerprint = %fingerprint, error = %e, "BiometricVerify rejected: signature verification failed");
                record_history(db, &fingerprint, connection_method::BIOMETRIC, connection_result::FAILED, Some(&address)).await;
                return Ok(Some(Message::error_with_id(&request_message_id, "SIGNATURE_INVALID", "Signature verification failed")));
            }

            // 4. 验签通过：签发 JWT 并完成认证
            let device_name = payload.device_name.clone().unwrap_or_else(|| pairing.device_name.clone());
            let session_token = jwt_service.generate_token(
                pairing.id.clone(),
                Some(device_name.clone()),
                Some(fingerprint.clone()),
            ).map_err(|e| crate::AppError::Auth(e.to_string()))?;

            ws_manager.set_authenticated(&addr, Some(pairing.id.clone()), Some(fingerprint.clone())).await;
            if let Some(ref name) = payload.device_name {
                ws_manager.set_device_name(&addr, Some(name.clone())).await;
            }

            record_history(db, &fingerprint, connection_method::BIOMETRIC, connection_result::SUCCESS, Some(&address)).await;

            if let Some(handle) = app_handle {
                let _ = handle.emit(event::DEVICE_CONNECTED, &DeviceConnectionEvent {
                    addr: address.clone(),
                    device_id: pairing.id.clone(),
                    device_name: payload.device_name.clone(),
                    fingerprint: Some(fingerprint.clone()),
                    event: "authenticated".to_string(),
                });
            }

            tracing::info!(pairing_id = %pairing.id, addr = %address, "Device authenticated via biometric");

            Ok(Some(Message::Auth {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: AuthPayload {
                    stage: AuthStage::Authenticated,
                    device_id: Some(pairing.id),
                    device_fingerprint: Some(fingerprint),
                    session_token: Some(session_token),
                    auth_method: Some(connection_method::BIOMETRIC.to_string()),
                    error: None,
                    ..Default::default()
                },
            }))
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
                let _ = handle.emit(event::DEVICE_CONNECTED, &event);
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
            tracing::warn!(addr = %addr, error = ?e, "JWT verification failed");

            // 返回认证失败响应
            let error_msg = match e {
                crate::utils::auth::JwtError::TokenExpired => "JWT token expired, please re-authenticate",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::path::Path;

    // ==================== 可测性说明 ====================
    // handle_auth 的 BiometricRequest / BiometricVerify 分支依赖
    // AppContext::global()（Tauri 应用全局状态，测试环境无法初始化），
    // 故未覆盖；其余分支（RequestPairing / VerifyCode / QrConnect）可脱离
    // Tauri 运行时验证（app_handle 传 None 跳过事件发射，WebSocketManager
    // 单例对未注册地址的 set_authenticated 为无副作用 no-op）。

    fn test_addr() -> SocketAddr {
        "127.0.0.1:8080".parse().unwrap()
    }

    /// 内存数据库：避免测试产生临时文件，schema 完整可用
    fn test_db() -> Arc<Mutex<Database>> {
        let db = Database::new(Path::new(":memory:")).expect("in-memory db");
        db.init_schema().expect("init schema");
        Arc::new(Mutex::new(db))
    }

    fn new_pairing_service() -> Arc<PairingService> {
        Arc::new(PairingService::new())
    }

    fn new_qr_manager() -> Arc<QrTokenManager> {
        Arc::new(QrTokenManager::new())
    }

    /// 解构 Auth 消息，提取断言所需的字段
    fn unwrap_auth(msg: Message) -> (String, AuthStage, AuthPayload) {
        match msg {
            Message::Auth { message_id, expect_response, payload, .. } => {
                assert!(!expect_response, "auth 响应不应要求客户端再回应");
                (message_id, payload.stage.clone(), payload)
            }
            other => panic!("expected Message::Auth, got {:?}", other),
        }
    }

    // ==================== format_device_display_name ====================

    #[test]
    fn test_display_name_extracts_ip_with_port() {
        assert_eq!(
            format_device_display_name("My Phone", "192.168.1.5:8080"),
            "My Phone (192.168.1.5)"
        );
    }

    #[test]
    fn test_display_name_ipv6_with_port() {
        // IPv6 地址带端口时，rsplit_once 只切最后一个冒号，括号保留
        assert_eq!(
            format_device_display_name("Phone", "[fe80::1]:8080"),
            "Phone ([fe80::1])"
        );
    }

    #[test]
    fn test_display_name_without_port_keeps_address() {
        assert_eq!(
            format_device_display_name("Phone", "myhost"),
            "Phone (myhost)"
        );
    }

    // ==================== handle_jwt_auth ====================

    #[tokio::test]
    async fn test_jwt_auth_missing_token_returns_failed() {
        // 未携带 session_token → 直接失败，无需 JWT 服务参与
        let payload = AuthPayload {
            stage: AuthStage::Reauthenticate,
            device_id: Some("dev-1".to_string()),
            ..Default::default()
        };
        let result = handle_jwt_auth(
            "msg-1".to_string(),
            Some("sess-1".to_string()),
            12345,
            payload,
            test_addr(),
            &JwtService::new(),
            &None,
        )
        .await
        .unwrap()
        .unwrap();

        let (message_id, stage, resp) = unwrap_auth(result);
        assert_eq!(message_id, "msg-1");
        assert_eq!(stage, AuthStage::Failed);
        assert_eq!(resp.error.as_deref(), Some("No JWT token provided"));
    }

    #[tokio::test]
    async fn test_jwt_auth_valid_token_authenticates() {
        let jwt = JwtService::new();
        let token = jwt
            .generate_token(
                "device-123".to_string(),
                Some("My Phone".to_string()),
                Some("fp-1".to_string()),
            )
            .unwrap();
        let payload = AuthPayload {
            stage: AuthStage::Reauthenticate,
            device_id: Some("device-123".to_string()),
            session_token: Some(token.clone()),
            ..Default::default()
        };
        let result = handle_jwt_auth(
            "msg-2".to_string(),
            None,
            12345,
            payload,
            test_addr(),
            &jwt,
            &None,
        )
        .await
        .unwrap()
        .unwrap();

        let (message_id, stage, resp) = unwrap_auth(result);
        assert_eq!(message_id, "msg-2");
        assert_eq!(stage, AuthStage::Authenticated);
        assert_eq!(resp.device_id.as_deref(), Some("device-123"));
        assert_eq!(resp.device_name.as_deref(), Some("My Phone"));
        assert_eq!(resp.device_fingerprint.as_deref(), Some("fp-1"));
        // 成功响应必须回传原 token，供客户端续用
        assert_eq!(resp.session_token.as_deref(), Some(token.as_str()));
        assert!(resp.error.is_none());
    }

    #[tokio::test]
    async fn test_jwt_auth_invalid_token_returns_failed() {
        let payload = AuthPayload {
            stage: AuthStage::Reauthenticate,
            session_token: Some("not-a-jwt".to_string()),
            ..Default::default()
        };
        let result = handle_jwt_auth(
            "msg-3".to_string(),
            None,
            12345,
            payload,
            test_addr(),
            &JwtService::new(),
            &None,
        )
        .await
        .unwrap()
        .unwrap();

        let (_, stage, resp) = unwrap_auth(result);
        assert_eq!(stage, AuthStage::Failed);
        assert_eq!(resp.error.as_deref(), Some("Invalid JWT token"));
    }

    #[tokio::test]
    async fn test_jwt_auth_expired_token_returns_failed() {
        let jwt = JwtService::with_expiry(1);
        let token = jwt.generate_token("device-123".to_string(), None, None).unwrap();
        // exp 以秒级截断，睡 2.1s 确保越过 exp 边界（与 jwt.rs 既有测试口径一致）
        std::thread::sleep(std::time::Duration::from_millis(2100));

        let payload = AuthPayload {
            stage: AuthStage::Reauthenticate,
            session_token: Some(token),
            ..Default::default()
        };
        let result = handle_jwt_auth(
            "msg-4".to_string(),
            None,
            12345,
            payload,
            test_addr(),
            &jwt,
            &None,
        )
        .await
        .unwrap()
        .unwrap();

        let (_, stage, resp) = unwrap_auth(result);
        assert_eq!(stage, AuthStage::Failed);
        assert_eq!(
            resp.error.as_deref(),
            Some("JWT token expired, please re-authenticate")
        );
    }

    // ==================== handle_auth: RequestPairing ====================

    #[tokio::test]
    async fn test_request_pairing_returns_verify_code_stage() {
        let pairing_service = new_pairing_service();
        let payload = AuthPayload {
            stage: AuthStage::RequestPairing,
            device_id: Some("dev-1".to_string()),
            device_name: Some("Phone".to_string()),
            ..Default::default()
        };
        let result = handle_auth(
            payload,
            "msg-rp".to_string(),
            test_addr(),
            &pairing_service,
            &new_qr_manager(),
            &JwtService::new(),
            WebSocketManager::global(),
            &None,
            &test_db(),
        )
        .await
        .unwrap()
        .unwrap();

        let (message_id, stage, resp) = unwrap_auth(result);
        assert_eq!(message_id, "msg-rp");
        // 配对请求的响应必须引导客户端进入 VerifyCode 阶段
        assert_eq!(stage, AuthStage::VerifyCode);
        assert_eq!(resp.device_id.as_deref(), Some("dev-1"));
        assert_eq!(resp.device_name.as_deref(), Some("Phone"));
        assert!(resp.error.is_none());

        // 配对码应已在服务端生成，且为 6 位数字
        let code = pairing_service.get_current_code().await.expect("code generated");
        assert_eq!(code.code.len(), 6);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    }

    // ==================== handle_auth: VerifyCode ====================

    #[tokio::test]
    async fn test_verify_code_success_issues_session_token() {
        let pairing_service = new_pairing_service();
        let code = pairing_service.generate_code().await;
        let payload = AuthPayload {
            stage: AuthStage::VerifyCode,
            device_id: Some("dev-1".to_string()),
            device_name: Some("Phone".to_string()),
            device_fingerprint: Some("fp-x".to_string()),
            pairing_code: Some(code.code.clone()),
            ..Default::default()
        };
        let result = handle_auth(
            payload,
            "msg-vc".to_string(),
            test_addr(),
            &pairing_service,
            &new_qr_manager(),
            &JwtService::new(),
            WebSocketManager::global(),
            &None,
            &test_db(),
        )
        .await
        .unwrap()
        .unwrap();

        let (message_id, stage, resp) = unwrap_auth(result);
        assert_eq!(message_id, "msg-vc");
        assert_eq!(stage, AuthStage::Authenticated);
        assert_eq!(resp.device_id.as_deref(), Some("dev-1"));
        assert_eq!(resp.device_fingerprint.as_deref(), Some("fp-x"));
        // 成功路径必须签发非空 JWT session token
        let token = resp.session_token.expect("session token issued");
        assert!(!token.is_empty());
        assert!(resp.error.is_none());
        // 配对码单次使用：验证后必须被消耗
        assert!(pairing_service.get_current_code().await.is_none());
    }

    #[tokio::test]
    async fn test_verify_code_failure_no_code_available() {
        // 服务端从未生成过配对码 → 明确提示先生成
        let payload = AuthPayload {
            stage: AuthStage::VerifyCode,
            pairing_code: Some("123456".to_string()),
            ..Default::default()
        };
        let result = handle_auth(
            payload,
            "msg-vc2".to_string(),
            test_addr(),
            &new_pairing_service(),
            &new_qr_manager(),
            &JwtService::new(),
            WebSocketManager::global(),
            &None,
            &test_db(),
        )
        .await
        .unwrap()
        .unwrap();

        let (_, stage, resp) = unwrap_auth(result);
        assert_eq!(stage, AuthStage::Failed);
        assert_eq!(
            resp.error.as_deref(),
            Some("No pairing code available. Please generate a new code.")
        );
    }

    #[tokio::test]
    async fn test_verify_code_failure_wrong_code() {
        let pairing_service = new_pairing_service();
        let code = pairing_service.generate_code().await;
        // 生成码与字面量冲突的概率为 10^-6，做一个翻转保证必不同
        let wrong = if code.code == "000000" { "111111" } else { "000000" };
        let payload = AuthPayload {
            stage: AuthStage::VerifyCode,
            pairing_code: Some(wrong.to_string()),
            ..Default::default()
        };
        let result = handle_auth(
            payload,
            "msg-vc3".to_string(),
            test_addr(),
            &pairing_service,
            &new_qr_manager(),
            &JwtService::new(),
            WebSocketManager::global(),
            &None,
            &test_db(),
        )
        .await
        .unwrap()
        .unwrap();

        let (_, stage, resp) = unwrap_auth(result);
        assert_eq!(stage, AuthStage::Failed);
        assert_eq!(resp.error.as_deref(), Some("Invalid or expired pairing code"));
        // 错误码不消耗配对码，正确码仍可重试
        assert!(pairing_service.get_current_code().await.is_some());
    }

    // ==================== handle_auth: QrConnect ====================

    #[tokio::test]
    async fn test_qr_connect_success() {
        let qr_manager = new_qr_manager();
        let token = qr_manager.generate(300).await;
        let payload = AuthPayload {
            stage: AuthStage::QrConnect,
            qr_token: Some(token),
            device_id: Some("qr-dev".to_string()),
            device_name: Some("QR Phone".to_string()),
            device_fingerprint: Some("fp-qr".to_string()),
            ..Default::default()
        };
        let result = handle_auth(
            payload,
            "msg-qr".to_string(),
            test_addr(),
            &new_pairing_service(),
            &qr_manager,
            &JwtService::new(),
            WebSocketManager::global(),
            &None,
            &test_db(),
        )
        .await
        .unwrap()
        .unwrap();

        let (message_id, stage, resp) = unwrap_auth(result);
        assert_eq!(message_id, "msg-qr");
        assert_eq!(stage, AuthStage::Authenticated);
        assert_eq!(resp.device_id.as_deref(), Some("qr-dev"));
        assert_eq!(resp.device_name.as_deref(), Some("QR Phone"));
        let session = resp.session_token.expect("session token issued");
        assert!(!session.is_empty());
        // 一次性 token 验证后必须被消耗
        assert!(qr_manager.get_active().await.is_none());
    }

    #[tokio::test]
    async fn test_qr_connect_without_token_fails() {
        // 桌面端未生成过二维码 → 返回 QrFailed 与中文引导提示
        let payload = AuthPayload {
            stage: AuthStage::QrConnect,
            qr_token: Some("anything".to_string()),
            ..Default::default()
        };
        let result = handle_auth(
            payload,
            "msg-qr2".to_string(),
            test_addr(),
            &new_pairing_service(),
            &new_qr_manager(),
            &JwtService::new(),
            WebSocketManager::global(),
            &None,
            &test_db(),
        )
        .await
        .unwrap()
        .unwrap();

        match result {
            Message::Auth { message_id, payload, .. } => {
                assert_eq!(message_id, "msg-qr2");
                assert_eq!(payload.stage, AuthStage::QrFailed);
                assert_eq!(payload.error.as_deref(), Some("请先在桌面端生成二维码"));
            }
            other => panic!("expected Message::Auth, got {:?}", other),
        }
    }
}

