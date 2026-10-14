//! 配对 / 认证编排域（票 14 阶段 B 自宿主 `commands::auth` 编排面迁入）
//!
//! **流程顺序、事件发射与拒绝文案归本插件；密码学与凭据在宿主**（WIT
//! `host-auth`，C4 凭据零过境——本域拿不到 token，认证成功后 JWT 由宿主
//! `apply_auth_success` 落地 global token + 凭据表；前端的 localStorage 持久化
//! 镜像经宿主窄读命令 `ws_get_auth_credentials` 取数，不经本插件）。
//!
//! 事件名与退役前的宿主命令面发射**逐字一致**（`ws_pairing_request` /
//! `ws_pairing_verified` / `ws_paired` / `ws_auth_failed`，载荷形状同）——
//! 经 host-events 透传给前端，监听端零改动。
//!
//! 与终端域的关系：同属「远程终端控制端」的连接侧编排（D6 选项 A 同 app
//! 分域）；本域不触达终端协议状态机（`link.rs`）。

use bedcode_plugin_api_mobile::host::{HostAuth, HostEvents, HostLog};
use bedcode_plugin_api_mobile::wasm_host::WasmHost;

/// 验证被桌面端业务拒绝时的固定文案（与退役前宿主命令面发射逐字一致）
const PAIRING_VERIFY_FAILED_REASON: &str = "Pairing verification failed";

/// 请求配对：桌面端生成一次性配对码并展示 → 广播 `ws_pairing_request`
pub(crate) fn request_pairing(h: &WasmHost) -> anyhow::Result<()> {
    h.auth_request_pairing().map_err(|e| anyhow::anyhow!(e.message))?;
    h.emit_event("ws_pairing_request", &serde_json::Value::Null);
    Ok(())
}

/// 验证配对码：受理 → `ws_pairing_verified` + `ws_paired`；业务拒绝 →
/// `ws_auth_failed`（固定文案，与退役前一致）；网络故障 → Err 上抛
pub(crate) fn verify_pairing_code(h: &WasmHost, code: &str) -> anyhow::Result<bool> {
    let accepted = h
        .auth_verify_pairing_code(code)
        .map_err(|e| anyhow::anyhow!(e.message))?;
    emit_outcome(h, accepted, PAIRING_VERIFY_FAILED_REASON);
    Ok(accepted)
}

/// QR token 认证：结果语义同 [`verify_pairing_code`]（拒绝不广播失败事件，
/// 与退役前宿主行为一致——QR 流程由前端直接提示）
pub(crate) fn authenticate_with_qr(h: &WasmHost, token: &str) -> anyhow::Result<bool> {
    let accepted = h
        .auth_qr_connect(token)
        .map_err(|e| anyhow::anyhow!(e.message))?;
    if accepted {
        emit_success(h);
    }
    Ok(accepted)
}

/// 生物认证登录（挑战-应答 + Keystore 签名全在宿主）：结果语义同
/// [`authenticate_with_qr`]
pub(crate) fn authenticate_with_biometric(h: &WasmHost) -> anyhow::Result<bool> {
    let accepted = h
        .auth_biometric_authenticate()
        .map_err(|e| anyhow::anyhow!(e.message))?;
    if accepted {
        emit_success(h);
    }
    Ok(accepted)
}

/// 激活期对账：事件不重放，认证状态以引擎事实为准（宿主是否持有 JWT）。
/// 仅观测日志，不派生对外状态
pub(crate) fn log_auth_state(h: &WasmHost) {
    match h.auth_has_credentials() {
        Ok(true) => h.log_info("auth state: host holds credentials (JWT)"),
        Ok(false) => h.log_info("auth state: no credentials held by host"),
        Err(e) => h.log_warn(&format!("auth state probe failed: {}", e.message)),
    }
}

/// 成功广播：`ws_pairing_verified` + `ws_paired`（前端 DevicesView 据后者
/// 刷新已配对态与会话列表）
fn emit_success(h: &WasmHost) {
    h.emit_event("ws_pairing_verified", &serde_json::Value::Null);
    h.emit_event("ws_paired", &serde_json::Value::Null);
}

/// 受理 / 拒绝的统一事件出口；拒绝是否广播失败事件由调用方语义决定
fn emit_outcome(h: &WasmHost, accepted: bool, reject_reason: &str) {
    if accepted {
        emit_success(h);
    } else {
        h.emit_event("ws_auth_failed", &serde_json::json!({ "reason": reject_reason }));
    }
}
