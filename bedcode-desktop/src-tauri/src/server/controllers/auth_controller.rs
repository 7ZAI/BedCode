//! Auth Controller
//!
//! HTTP REST API endpoints for authentication
//! Routes:
//! - POST /api/auth/pairing
//! - POST /api/auth/verify
//! - POST /api/auth/qr-connect
//! - POST /api/auth/reauth

use actix_web::{web, HttpResponse};
use tauri::Emitter;
use crate::system::app_context::AppContext;
use crate::server::dtos::ApiResponse;
use crate::server::dtos::auth_dto::*;
use crate::utils::auth::jwt::JwtService;
use crate::utils::auth::jwt::DEFAULT_TOKEN_EXPIRY_SECS;
use crate::server::services::auth_service::format_device_display_name;

/// POST /api/auth/pairing
///
/// 请求配对码，桌面端弹出配对码供移动端输入
pub async fn request_pairing(
    body: web::Json<PairingRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let pairing_service = ctx.pairing_service();
    let app_handle = ctx.app_handle();

    let code = pairing_service.generate_code().await;

    // 通知桌面端前端显示配对码
    if let Err(e) = app_handle.emit("pairing-code-generated", &crate::server::connection_types::PairingCodeGeneratedEvent {
        code: code.code.clone(),
        expires_in: code.remaining_seconds(),
        device_name: Some(body.device_name.clone()),
    }) {
        tracing::error!(error = %e, "Failed to emit pairing code event");
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
pub async fn verify_pairing_code(
    body: web::Json<VerifyPairingRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let pairing_service = ctx.pairing_service();

    let is_valid = pairing_service.verify_and_consume_code(&body.pairing_code).await;

    if !is_valid {
        tracing::warn!(device_name = ?body.device_name, "Pairing code verification failed");
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

    // 通知桌面端有设备连接
    let app_handle = ctx.app_handle();
    let _ = app_handle.emit("device-connected", &crate::server::connection_types::DeviceConnectionEvent {
        addr: body.address.clone(),
        device_id: body.device_id.clone(),
        device_name: Some(body.device_name.clone()),
        fingerprint: Some(body.fingerprint.clone()),
        event: "authenticated".to_string(),
    });

    let data = AuthTokenResponseData {
        expires_in: DEFAULT_TOKEN_EXPIRY_SECS,
        token,
    };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/auth/qr-connect
///
/// QR 码认证，成功返回 JWT token
pub async fn qr_connect(
    body: web::Json<QrConnectRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let qr_manager = ctx.qr_manager();

    match qr_manager.verify(&body.qr_token).await {
        Ok(()) => {
            let app_handle = ctx.app_handle();
            let _ = app_handle.emit("qr-token-consumed", ());

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

            let _ = app_handle.emit("device-connected", &crate::server::connection_types::DeviceConnectionEvent {
                addr: address,
                device_id,
                device_name: Some(device_name),
                fingerprint: Some(fingerprint),
                event: "authenticated".to_string(),
            });

            let data = AuthTokenResponseData {
                expires_in: DEFAULT_TOKEN_EXPIRY_SECS,
                token,
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::warn!(error = %e, "QR token verification failed");
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
pub async fn reauthenticate(
    body: web::Json<ReauthRequest>,
) -> HttpResponse {
    let jwt_service = JwtService::new();

    match jwt_service.verify_token_with_expiry(&body.session_token) {
        Ok(claims) => {
            // 更新配对设备的 last_seen 和 connect_count
            if let Some(ref fp) = claims.fingerprint {
                let ctx = AppContext::global();
                let db = ctx.db();
                let db_guard = db.lock().await;
                if let Err(e) = db_guard.update_pairing_last_seen(fp) {
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
            let msg = match e {
                crate::utils::auth::jwt::JwtError::TokenExpired => "Token expired",
                _ => "Invalid token",
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(1001, msg))
        }
    }
}
