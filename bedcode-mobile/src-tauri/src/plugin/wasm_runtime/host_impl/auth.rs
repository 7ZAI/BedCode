//! host-auth —— 设备入场认证引擎面（逻辑层，ABI v16 · 票 14 阶段 B）
//!
//! **编排在消费插件，密码学与凭据在宿主**（C4 红线）：设备身份文件 / JWT
//! 持有 / `AuthHttpClient` / 生物凭证绑定与 Keystore 签名全部留在 auth 引擎
//! （`auth/manager.rs` / `auth/http.rs`），本模块只把引擎调用投影给插件，
//! 流程顺序（请求配对 → 等码 → 验码 / QR / 生物挑战）、事件发射与状态派生
//! 归插件。**凭据零过境**——本域任何函数都不向插件返回凭据材料（对齐
//! 票 12 `jwt-auth`「token 不落插件」先例）；前端持久化镜像走宿主窄读命令
//! `ws_get_auth_credentials`，不经插件。
//!
//! 权限位 `auth`（fail-closed，未声明即拒——与 ws:client 同语义）。
//!
//! 同步阻塞语义：host fn 在同步上下文执行，HTTP 往返阻塞本插件实例直至
//! 完成或超时（与 `host-websocket.connect` 同款裁决，票 11）。

use super::super::{WasmPluginState, block_on_async};

/// 权限门（manifest `granted_permissions` 仲裁，与 ws:client 同语义）
fn check_permission(state: &WasmPluginState) -> bool {
    state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_AUTH)
}

fn denied() -> String {
    format!(
        "permission denied: {}",
        bedcode_plugin_api_mobile::permission::PERMISSION_AUTH
    )
}

// ==================== 认证引擎原语 ====================

/// 请求配对（HTTP `POST /api/auth/request-pairing`，桌面端出码）
pub(crate) fn auth_request_pairing(state: &WasmPluginState) -> Result<(), String> {
    if !check_permission(state) {
        return Err(denied());
    }
    let plugin_id = state.plugin_id.clone();
    let auth = crate::state::get_auth_manager();
    block_on_async(&state.runtime_handle, async move { auth.request_pairing().await })
        .inspect(|_| {
            tracing::info!(plugin_id = %plugin_id, "host-auth: pairing requested (desktop issued code)");
        })
        .map_err(|e| {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: request-pairing failed");
            e.to_string()
        })
}

/// 验证配对码（HTTP `POST /api/auth/verify-pairing-code`）
///
/// Ok(true) = 受理且凭据已落地宿主（`apply_auth_success`）；Ok(false) =
/// 桌面端业务拒绝；Err = 网络故障。凭据留在宿主，不向插件返回
pub(crate) fn auth_verify_pairing_code(state: &WasmPluginState, code: &str) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    let plugin_id = state.plugin_id.clone();
    let auth = crate::state::get_auth_manager();
    let code = code.to_string();
    block_on_async(&state.runtime_handle, async move { auth.verify_pairing_code(&code).await })
        .inspect(|accepted| {
            tracing::info!(plugin_id = %plugin_id, accepted = accepted, "host-auth: pairing code verified");
        })
        .map_err(|e| {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: verify-pairing-code failed");
            e.to_string()
        })
}

/// QR token 认证（HTTP `POST /api/auth/qr-connect`）
pub(crate) fn auth_qr_connect(state: &WasmPluginState, token: &str) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    let plugin_id = state.plugin_id.clone();
    let auth = crate::state::get_auth_manager();
    let token = token.to_string();
    block_on_async(&state.runtime_handle, async move { auth.authenticate_with_qr(&token).await })
        .inspect(|accepted| {
            tracing::info!(plugin_id = %plugin_id, accepted = accepted, "host-auth: qr connect done");
        })
        .map_err(|e| {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: qr-connect failed");
            e.to_string()
        })
}

/// 生物认证登录（HTTP 挑战-应答 + Keystore 签名，全在宿主执行）
pub(crate) fn auth_biometric_authenticate(state: &WasmPluginState) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    let plugin_id = state.plugin_id.clone();
    let auth = crate::state::get_auth_manager();
    block_on_async(&state.runtime_handle, async move { auth.authenticate_with_biometric().await })
        .inspect(|accepted| {
            tracing::info!(plugin_id = %plugin_id, accepted = accepted, "host-auth: biometric authenticate done");
        })
        .map_err(|e| {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: biometric-authenticate failed");
            e.to_string()
        })
}

/// 引擎事实：宿主当前是否持有认证凭据（JWT）
pub(crate) fn auth_has_credentials(state: &WasmPluginState) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    let auth = crate::state::get_auth_manager();
    Ok(block_on_async(&state.runtime_handle, async move { auth.get_credentials().await }).is_some())
}
