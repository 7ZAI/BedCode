//! Mobile Auth Commands
//!
//! 认证与配对相关命令（票 14 阶段 B 收窄后的残余引擎面）
//!
//! 配对流程编排（请求配对 → 等码 → 验码 / QR / 生物挑战）已迁
//! `com.bedcode.terminal-session` 插件（经 WIT `host-auth` 触达引擎，凭据
//! 零过境）；本文件只剩三类**引擎事实 / 凭据面**（C4：凭据持有在宿主引擎，
//! 读取面也留在引擎侧，不经插件）：
//!
//! - `ws_authenticate`：重启 / 重连后的 JWT 换新（token 由前端 localStorage
//!   镜像交还宿主——重启后宿主内存态 global token 为空的恢复路径）
//! - `ws_get_auth_credentials`：凭据窄读（前端持久化镜像的唯一取数口）
//! - 生物凭证绑定 / 解绑 / 状态（Keystore 私钥与公钥注册留在宿主引擎）

use tauri::AppHandle;

use crate::auth::AuthCredentials;
use crate::router::event;
use crate::state::get_auth_manager;
use crate::Result;

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

/// 读取宿主持有的认证凭据（窄读引擎事实）
///
/// 票 14 阶段 B：认证编排迁插件后，配对 / QR / 生物认证的成功路径不再向
/// 前端返回凭据（凭据零过境，插件不接触 token）；前端持久化镜像
/// （localStorage，重启后经 `ws_authenticate` 交还宿主）经本命令直接读引擎。
/// 未持有凭据 → `None`
#[tauri::command]
pub async fn ws_get_auth_credentials() -> Result<Option<AuthCredentials>> {
    let auth = get_auth_manager();
    Ok(auth.get_credentials().await)
}

/// 绑定生物凭证：本地生成密钥对并注册公钥到桌面端（需已认证连接）
#[tauri::command]
pub async fn ws_bind_biometric_credential() -> Result<bool> {
    let auth = get_auth_manager();
    auth.bind_biometric_credential().await
}

/// 解绑生物凭证：删除本地密钥并通知桌面端清空公钥（需已认证连接）
#[tauri::command]
pub async fn ws_unbind_biometric_credential() -> Result<bool> {
    let auth = get_auth_manager();
    auth.unbind_biometric_credential().await
}

/// 生物认证密钥状态（设备支持 + 本地密钥已生成）
///
/// camelCase 序列化与前端 TS 接口对齐（同 commands/session.rs 约定）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricKeyStatus {
    pub device_supported: bool,
    /// BiometricManager 结果码：0=SUCCESS 1=HW_UNAVAILABLE 11=NONE_ENROLLED 12=NO_HARDWARE；-1=未知/插件异常
    pub device_reason: i32,
    pub has_key: bool,
}

/// 查询生物认证密钥状态
#[tauri::command]
pub async fn ws_get_biometric_key_status() -> Result<BiometricKeyStatus> {
    let auth = get_auth_manager();
    let (device_supported, device_reason) = auth.is_biometric_supported().await?;
    let has_key = auth.has_biometric_key().await?;
    Ok(BiometricKeyStatus {
        device_supported,
        device_reason,
        has_key,
    })
}
