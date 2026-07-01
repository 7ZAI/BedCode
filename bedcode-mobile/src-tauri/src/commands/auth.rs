//! Mobile Auth Commands
//!
//! 认证和配对相关命令

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::Result;
use crate::auth::{AuthCredentials, AuthStatus};
use crate::router::event;
use crate::state::get_auth_manager;

/// 认证状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub status: String,
    pub is_authenticated: bool,
}

/// 获取认证状态
#[tauri::command]
pub async fn ws_get_auth_status() -> Result<AuthState> {
    let auth = get_auth_manager();
    let status = auth.get_status().await;
    let is_authenticated = matches!(status, AuthStatus::Authenticated);

    Ok(AuthState {
        status: format!("{:?}", status),
        is_authenticated,
    })
}

/// 使用 JWT token 认证（重连时使用已存储的 session_token）
#[tauri::command]
pub async fn ws_authenticate(app_handle: AppHandle, session_token: String) -> Result<bool> {
    tracing::info!("[ws_authenticate] called, token length={}", session_token.len());
    let auth = get_auth_manager();
    let result = auth.authenticate_with_token(&session_token).await?;

    if result {
        event::emit_auth_success(&app_handle);
        event::emit_paired(&app_handle);
    }

    Ok(result)
}

/// 请求配对
#[tauri::command]
pub async fn ws_request_pairing(app_handle: AppHandle) -> Result<()> {
    eprintln!("[ws_request_pairing] COMMAND ENTERED!");
    tracing::info!("[ws_request_pairing] command entered");

    let auth = get_auth_manager();
    tracing::info!("[ws_request_pairing] got auth manager, calling request_pairing...");
    match auth.request_pairing().await {
        Ok(()) => {
            tracing::info!("[ws_request_pairing] request_pairing OK, emitting event");
            event::emit_pairing_request(&app_handle);
            Ok(())
        }
        Err(e) => {
            tracing::error!("[ws_request_pairing] request_pairing failed: {}", e);
            Err(e)
        }
    }
}

/// 验证配对码，成功后返回凭据（含 JWT token）
#[tauri::command]
pub async fn ws_verify_pairing_code(app_handle: AppHandle, code: String) -> Result<Option<AuthCredentials>> {
    let auth = get_auth_manager();
    let result = auth.verify_pairing_code(&code).await?;

    if result {
        event::emit_pairing_verified(&app_handle);
        event::emit_paired(&app_handle);
        // 返回存储的凭据，前端持久化到 localStorage
        Ok(auth.get_credentials().await)
    } else {
        event::emit_auth_failed(&app_handle, "Pairing verification failed");
        Ok(None)
    }
}

/// 使用 QR token 认证
#[tauri::command]
pub async fn ws_authenticate_with_qr(app_handle: AppHandle, token: String) -> Result<Option<AuthCredentials>> {
    let auth = get_auth_manager();
    let result = auth.authenticate_with_qr(&token).await?;

    if result {
        event::emit_pairing_verified(&app_handle);
        event::emit_paired(&app_handle);
        return Ok(auth.get_credentials().await);
    }

    Ok(None)
}