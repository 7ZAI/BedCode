//! Auth Controller
//!
//! HTTP REST API endpoints for authentication
//! Routes:
//! - POST /api/auth/pairing
//! - POST /api/auth/verify
//! - POST /api/auth/qr-connect
//! - POST /api/auth/reauth
//! - POST /api/auth/biometric-challenge
//! - POST /api/auth/biometric-verify

use crate::server::dtos::auth_dto::*;
use crate::server::dtos::ApiResponse;
use crate::server::services::auth_service::{
    format_device_display_name, issue_biometric_challenge, verify_biometric_challenge, BiometricAuthError,
};
use crate::system::app_context::AppContext;
use crate::system::constants::event;
use crate::utils::auth::biometric::BIO_CHALLENGE_TTL_SECS;
use crate::utils::auth::jwt::JwtService;
use crate::utils::auth::jwt::DEFAULT_TOKEN_EXPIRY_SECS;
use actix_web::{web, HttpResponse};
use tauri::Emitter;

/// POST /api/auth/pairing
///
/// 请求配对码，桌面端弹出配对码供移动端输入
pub async fn request_pairing(body: web::Json<PairingRequest>) -> HttpResponse {
    let ctx = AppContext::global();
    let pairing_service = ctx.pairing_service();

    let code = pairing_service.generate_code().await;

    // 通知桌面端前端显示配对码（无头/测试上下文无 AppHandle：跳过）
    if let Some(handle) = ctx.app_handle() {
        if let Err(e) = handle.emit(
            "pairing-code-generated",
            &crate::server::connection_types::PairingCodeGeneratedEvent {
                code: code.code.clone(),
                expires_in: code.remaining_seconds(),
                device_name: Some(body.device_name.clone()),
            },
        ) {
            tracing::error!(error = %e, "Failed to emit pairing code event");
        }
    }

    let data = PairingResponseData {
        pairing_code: code.code.clone(),
        expires_in: code.remaining_seconds(),
    };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/auth/verify
///
/// 验证配对码，成功返回 JWT token
pub async fn verify_pairing_code(body: web::Json<VerifyPairingRequest>) -> HttpResponse {
    let ctx = AppContext::global();
    let pairing_service = ctx.pairing_service();

    let is_valid = pairing_service.verify_and_consume_code(&body.pairing_code).await;

    if !is_valid {
        tracing::warn!(device_name = ?body.device_name, "Pairing code verification failed");
        // 记录连接历史（配对码认证失败）
        {
            let db = ctx.db();
            let db_guard = db.lock().await;
            if let Err(e) = db_guard.record_connection_event_by_fingerprint(
                &body.fingerprint,
                crate::db::connection_method::PAIRING_CODE,
                crate::db::connection_result::FAILED,
                Some(&body.address),
            ) {
                tracing::warn!(error = %e, "Failed to record connection history");
            }
        }
        let current_code = pairing_service.get_current_code().await;
        let msg = if current_code.is_none() {
            "No pairing code available. Please generate a new code."
        } else {
            "Invalid or expired pairing code"
        };
        return HttpResponse::Ok().json(ApiResponse::<()>::error(1005, msg));
    }

    let jwt_service = JwtService::new();
    let token = match jwt_service.generate_token(
        body.device_id.clone(),
        Some(body.device_name.clone()),
        Some(body.fingerprint.clone()),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "JWT generation failed");
            return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
        }
    };

    // 记录/更新配对设备到数据库
    let display_name = format_device_display_name(&body.device_name, &body.address);
    {
        let db = ctx.db();
        let db_guard = db.lock().await;
        if let Err(e) = db_guard.add_pairing(&display_name, &body.fingerprint, "", Some(&body.address)) {
            tracing::warn!(device_name = %body.device_name, error = %e, "Failed to record pairing");
        }
    }

    // 记录连接历史（配对码认证成功）
    {
        let db = ctx.db();
        let db_guard = db.lock().await;
        if let Err(e) = db_guard.record_connection_event_by_fingerprint(
            &body.fingerprint,
            crate::db::connection_method::PAIRING_CODE,
            crate::db::connection_result::SUCCESS,
            Some(&body.address),
        ) {
            tracing::warn!(error = %e, "Failed to record connection history");
        }
    }

    // 通知桌面端有设备连接（无头/测试上下文无 AppHandle：跳过）
    if let Some(handle) = ctx.app_handle() {
        let _ = handle.emit(
            event::DEVICE_CONNECTED,
            &crate::server::connection_types::DeviceConnectionEvent {
                addr: body.address.clone(),
                device_id: body.device_id.clone(),
                device_name: Some(body.device_name.clone()),
                fingerprint: Some(body.fingerprint.clone()),
                event: "authenticated".to_string(),
            },
        );
    }

    let data = AuthTokenResponseData {
        expires_in: DEFAULT_TOKEN_EXPIRY_SECS,
        token,
    };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/auth/qr-connect
///
/// QR 码认证，成功返回 JWT token
pub async fn qr_connect(body: web::Json<QrConnectRequest>) -> HttpResponse {
    let ctx = AppContext::global();
    let qr_manager = ctx.qr_manager();

    match qr_manager.verify(&body.qr_token).await {
        Ok(()) => {
            // 无头/测试上下文无 AppHandle：跳过前端事件
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit("qr-token-consumed", ());
            }

            let device_id = body.device_id.clone();
            let device_name = body.device_name.clone();
            let fingerprint = body.fingerprint.clone();
            let address = body.address.clone();

            let jwt_service = JwtService::new();
            let token = match jwt_service.generate_token(
                device_id.clone(),
                Some(device_name.clone()),
                Some(fingerprint.clone()),
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "JWT generation failed");
                    return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
                }
            };

            // 记录/更新配对设备到数据库
            let display_name = format_device_display_name(&device_name, &address);
            {
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.add_pairing(&display_name, &fingerprint, "", Some(&address)) {
                    tracing::warn!(device_name = %device_name, error = %e, "Failed to record pairing");
                }
            }

            // 记录连接历史（QR 认证成功）
            {
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.record_connection_event_by_fingerprint(
                    &fingerprint,
                    crate::db::connection_method::QR,
                    crate::db::connection_result::SUCCESS,
                    Some(&address),
                ) {
                    tracing::warn!(error = %e, "Failed to record connection history");
                }
            }

            // 无头/测试上下文无 AppHandle：跳过前端事件
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(
                    event::DEVICE_CONNECTED,
                    &crate::server::connection_types::DeviceConnectionEvent {
                        addr: address,
                        device_id,
                        device_name: Some(device_name),
                        fingerprint: Some(fingerprint),
                        event: "authenticated".to_string(),
                    },
                );
            }

            let data = AuthTokenResponseData {
                expires_in: DEFAULT_TOKEN_EXPIRY_SECS,
                token,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::warn!(error = %e, "QR token verification failed");
            // 记录连接历史（QR 认证失败）
            {
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.record_connection_event_by_fingerprint(
                    &body.fingerprint,
                    crate::db::connection_method::QR,
                    crate::db::connection_result::FAILED,
                    Some(&body.address),
                ) {
                    tracing::warn!(error = %e, "Failed to record connection history");
                }
            }
            let error_msg = e.to_string();
            let user_msg = if error_msg.contains("expired") {
                "二维码已过期，请重新生成"
            } else if error_msg.contains("already used") {
                "二维码已绑定其他设备，请重新扫描"
            } else if error_msg.contains("No active QR token") {
                "请先在桌面端生成二维码"
            } else {
                &error_msg
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1006, user_msg))
        }
    }
}

/// POST /api/auth/reauth
///
/// 使用已有 JWT token 重新认证
pub async fn reauthenticate(body: web::Json<ReauthRequest>) -> HttpResponse {
    let jwt_service = JwtService::new();

    match jwt_service.verify_token_with_expiry(&body.session_token) {
        Ok(claims) => {
            // 记录连接历史（JWT 静默重连成功）
            if let Some(ref fp) = claims.fingerprint {
                let ctx = AppContext::global();
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.record_connection_event_by_fingerprint(
                    fp,
                    crate::db::connection_method::JWT,
                    crate::db::connection_result::SUCCESS,
                    None,
                ) {
                    tracing::warn!(error = %e, "Failed to record connection history");
                }
            }

            // 更新配对设备的 last_seen 和 connect_count
            // （HTTP 重认证路径无客户端地址，无法拼展示名；设备名刷新由 WS 重认证路径承担）
            if let Some(ref fp) = claims.fingerprint {
                let ctx = AppContext::global();
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.update_pairing_last_seen(fp, None) {
                    tracing::warn!(fingerprint = %fp, error = %e, "Failed to update pairing last_seen");
                }
            }

            let new_token = match jwt_service.generate_token(
                claims.sub.clone(),
                claims.device_name.clone(),
                claims.fingerprint.clone(),
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "JWT generation failed");
                    return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
                }
            };

            let data = AuthTokenResponseData {
                expires_in: DEFAULT_TOKEN_EXPIRY_SECS,
                token: new_token,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Reauth JWT verification failed");
            // 记录连接历史（JWT 静默重连失败）
            {
                let ctx = AppContext::global();
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.record_connection_event_by_fingerprint(
                    &body.fingerprint,
                    crate::db::connection_method::JWT,
                    crate::db::connection_result::FAILED,
                    None,
                ) {
                    tracing::warn!(error = %e, "Failed to record connection history");
                }
            }
            let msg = match e {
                crate::utils::auth::jwt::JwtError::TokenExpired => "Token expired",
                _ => "Invalid token",
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1001, msg))
        }
    }
}

/// POST /api/auth/biometric-challenge
///
/// 生物认证挑战值下发：设备须已配对且绑定生物凭证公钥，返回一次性、
/// 60s 有效的挑战值（防重放）。失败返回 1008（不撞既有 1001/1005/1006/1007）。
/// 挑战以设备指纹为键——同一设备多条通道共享一次挑战，单次消费后作废
pub async fn biometric_challenge(body: web::Json<BiometricChallengeRequest>) -> HttpResponse {
    let ctx = AppContext::global();
    let fingerprint = body.device_fingerprint.clone();

    match issue_biometric_challenge(&fingerprint).await {
        Ok(nonce) => {
            let data = BiometricChallengeResponseData {
                challenge_nonce: nonce,
                expires_in: BIO_CHALLENGE_TTL_SECS,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::warn!(fingerprint = %fingerprint, error = ?e, "Biometric challenge issuance failed");
            // 记录连接历史（挑战签发失败 = 认证尝试失败；未配对指纹不落库）
            {
                let db_guard = ctx.db().lock().await;
                if let Err(err) = db_guard.record_connection_event_by_fingerprint(
                    &fingerprint,
                    crate::db::connection_method::BIOMETRIC,
                    crate::db::connection_result::FAILED,
                    // HTTP 端无客户端地址上下文（移动端地址由 WS 握手获得），
                    // 连接历史不记地址，展示与 WS 路径互补
                    None,
                ) {
                    tracing::warn!(error = %err, "Failed to record connection history");
                }
            }
            let msg = match e {
                // DB 故障也是挑战不可签发，归 1008 并携带原因便于排查
                BiometricAuthError::Database(err) => err,
                _ => "Biometric credential not bound".to_string(),
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1008, &msg))
        }
    }
}

/// POST /api/auth/biometric-verify
///
/// 生物认证验签：一次性消费挑战值 + 绑定公钥验签，通过后签发 JWT。
/// 失败返回 1009。⚠️ add_pairing 必须保留 public_key——既有 verify/qr 端点
/// 传空串会把生物凭证清空，这里传 pairing.public_key 防覆盖
pub async fn biometric_verify(body: web::Json<BiometricVerifyRequest>) -> HttpResponse {
    let ctx = AppContext::global();
    let fingerprint = body.device_fingerprint.clone();

    match verify_biometric_challenge(&fingerprint, &body.challenge_nonce, &body.signature).await {
        Ok(pairing) => {
            // 设备名取配对记录（HTTP 端点无自报名称，与 WS 路径的
            // payload.device_name.unwrap_or(pairing.device_name) 对齐）
            let device_name = pairing.device_name.clone();
            let jwt_service = JwtService::new();
            let token = match jwt_service.generate_token(
                pairing.id.clone(),
                Some(device_name.clone()),
                Some(fingerprint.clone()),
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "JWT generation failed");
                    return HttpResponse::Ok().json(ApiResponse::<()>::error(1001, "Failed to generate token"));
                }
            };

            // 刷新配对记录（connect_count / last_seen）——必须保留公钥防覆盖
            {
                let db_guard = ctx.db().lock().await;
                if let Err(e) = db_guard.add_pairing(&device_name, &fingerprint, &pairing.public_key, None) {
                    tracing::warn!(device_name = %device_name, error = %e, "Failed to record pairing");
                }
            }

            // 记录连接历史（生物认证成功）
            {
                let db_guard = ctx.db().lock().await;
                if let Err(e) = db_guard.record_connection_event_by_fingerprint(
                    &fingerprint,
                    crate::db::connection_method::BIOMETRIC,
                    crate::db::connection_result::SUCCESS,
                    None,
                ) {
                    tracing::warn!(error = %e, "Failed to record connection history");
                }
            }

            // 通知桌面端有设备连接（无头/测试上下文无 AppHandle：跳过）；
            // HTTP 端无客户端地址，addr 留空（与挑战/verify 的无地址语义一致）
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(
                    event::DEVICE_CONNECTED,
                    &crate::server::connection_types::DeviceConnectionEvent {
                        addr: String::new(),
                        device_id: pairing.id.clone(),
                        device_name: Some(device_name.clone()),
                        fingerprint: Some(fingerprint.clone()),
                        event: "authenticated".to_string(),
                    },
                );
            }

            tracing::info!(pairing_id = %pairing.id, fingerprint = %fingerprint, "Device authenticated via biometric (HTTP)");

            let data = AuthTokenResponseData {
                expires_in: DEFAULT_TOKEN_EXPIRY_SECS,
                token,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::warn!(fingerprint = %fingerprint, error = ?e, "Biometric verify failed");
            // 记录连接历史（验证失败）
            {
                let db_guard = ctx.db().lock().await;
                if let Err(err) = db_guard.record_connection_event_by_fingerprint(
                    &fingerprint,
                    crate::db::connection_method::BIOMETRIC,
                    crate::db::connection_result::FAILED,
                    None,
                ) {
                    tracing::warn!(error = %err, "Failed to record connection history");
                }
            }
            let msg = match e {
                BiometricAuthError::ChallengeInvalid(_) => "Biometric challenge invalid or expired".to_string(),
                BiometricAuthError::NotPaired => "Device not paired".to_string(),
                BiometricAuthError::CredentialNotBound => "Biometric credential not bound".to_string(),
                BiometricAuthError::SignatureInvalid(_) => "Biometric signature verification failed".to_string(),
                BiometricAuthError::Database(err) => err,
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1009, &msg))
        }
    }
}
